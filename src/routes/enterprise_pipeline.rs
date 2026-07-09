//! Recruiter pipeline kanban — Phase 3.5.

use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub const VALID_STAGES: &[&str] = &[
    "to_contact",
    "contacted",
    "interviewing",
    "offer_sent",
    "hired",
    "rejected",
    "dropped",
];

pub fn enterprise_pipeline_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/enterprise/pipeline",
            get(list_entries).post(add_entry),
        )
        .route(
            "/enterprise/pipeline/{id}",
            put(update_entry).delete(remove_entry),
        )
        .route("/enterprise/pipeline/export.csv", get(export_csv))
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

async fn current_enterprise_for(
    db: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Uuid, AppError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT enterprise_id FROM enterprise_members WHERE user_id = $1 AND status = 'active' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    row.map(|(id,)| id).ok_or(AppError::Forbidden)
}

fn check_stage(s: &str) -> Result<(), AppError> {
    if VALID_STAGES.contains(&s) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "invalid stage; allowed: {}",
            VALID_STAGES.join(", ")
        )))
    }
}

#[derive(Deserialize)]
struct ListQuery {
    stage: Option<String>,
}

async fn list_entries(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT p.id, p.talent_id, u.username, u.display_name, u.skill_domain, u.title, u.total_fragments,
               p.stage, p.position, p.notes, p.salary_proposed_eur, p.last_action_at, p.created_at, p.updated_at
        FROM enterprise_pipeline_entries p
        JOIN users u ON u.id = p.talent_id
        WHERE p.enterprise_id = $1
          AND ($2::text IS NULL OR p.stage = $2)
        ORDER BY p.stage, p.position, p.last_action_at DESC
        "#,
    )
    .bind(enterprise_id)
    .bind(&q.stage)
    .fetch_all(&state.db)
    .await?;
    use sqlx::Row;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "talent_id": r.get::<Uuid, _>("talent_id"),
                "username": r.get::<String, _>("username"),
                "display_name": r.get::<String, _>("display_name"),
                "skill_domain": r.get::<String, _>("skill_domain"),
                "title": r.get::<String, _>("title"),
                "total_fragments": r.get::<i32, _>("total_fragments"),
                "stage": r.get::<String, _>("stage"),
                "position": r.get::<i32, _>("position"),
                "notes": r.get::<Option<String>, _>("notes"),
                "salary_proposed_eur": r.get::<Option<i32>, _>("salary_proposed_eur"),
                "last_action_at": r.get::<chrono::DateTime<chrono::Utc>, _>("last_action_at"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "entries": items }))))
}

#[derive(Deserialize)]
struct AddEntryBody {
    talent_id: Uuid,
    stage: Option<String>,
    notes: Option<String>,
    salary_proposed_eur: Option<i32>,
}

async fn add_entry(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<AddEntryBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let stage = body.stage.unwrap_or_else(|| "to_contact".into());
    check_stage(&stage)?;
    let entry_id: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO enterprise_pipeline_entries
            (enterprise_id, talent_id, stage, notes, salary_proposed_eur, created_by)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (enterprise_id, talent_id) DO UPDATE SET
            stage = EXCLUDED.stage,
            notes = COALESCE(EXCLUDED.notes, enterprise_pipeline_entries.notes),
            salary_proposed_eur = COALESCE(EXCLUDED.salary_proposed_eur, enterprise_pipeline_entries.salary_proposed_eur),
            last_action_at = NOW(),
            updated_at = NOW()
        RETURNING id
        "#,
    )
    .bind(enterprise_id)
    .bind(body.talent_id)
    .bind(&stage)
    .bind(body.notes.as_deref())
    .bind(body.salary_proposed_eur)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;
    let _ = sqlx::query(
        "INSERT INTO enterprise_pipeline_history (entry_id, to_stage, actor_user_id) VALUES ($1, $2, $3)",
    )
    .bind(entry_id.0)
    .bind(&stage)
    .bind(auth.user_id)
    .execute(&state.db)
    .await;
    Ok(Json(build_response(json!({ "entry_id": entry_id.0 }))))
}

#[derive(Deserialize)]
struct UpdateEntryBody {
    stage: Option<String>,
    notes: Option<String>,
    salary_proposed_eur: Option<i32>,
    position: Option<i32>,
}

async fn update_entry(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateEntryBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT stage FROM enterprise_pipeline_entries WHERE id = $1 AND enterprise_id = $2",
    )
    .bind(id)
    .bind(enterprise_id)
    .fetch_optional(&state.db)
    .await?;
    let (from_stage,) = existing.ok_or(AppError::NotFound("pipeline entry not found".into()))?;
    if let Some(ref s) = body.stage {
        check_stage(s)?;
    }
    sqlx::query(
        r#"
        UPDATE enterprise_pipeline_entries SET
            stage = COALESCE($1, stage),
            notes = COALESCE($2, notes),
            salary_proposed_eur = COALESCE($3, salary_proposed_eur),
            position = COALESCE($4, position),
            last_action_at = NOW(),
            updated_at = NOW()
        WHERE id = $5 AND enterprise_id = $6
        "#,
    )
    .bind(body.stage.as_deref())
    .bind(body.notes.as_deref())
    .bind(body.salary_proposed_eur)
    .bind(body.position)
    .bind(id)
    .bind(enterprise_id)
    .execute(&state.db)
    .await?;
    if let Some(new_stage) = body.stage {
        if new_stage != from_stage {
            let _ = sqlx::query(
                "INSERT INTO enterprise_pipeline_history (entry_id, from_stage, to_stage, actor_user_id) VALUES ($1, $2, $3, $4)",
            )
            .bind(id)
            .bind(&from_stage)
            .bind(&new_stage)
            .bind(auth.user_id)
            .execute(&state.db)
            .await;
            if new_stage == "hired" {
                metrics::counter!("skilluv_pipeline_hires_total").increment(1);
            }
        }
    }
    Ok(Json(build_response(json!({ "updated": true }))))
}

async fn remove_entry(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    sqlx::query(
        "DELETE FROM enterprise_pipeline_entries WHERE id = $1 AND enterprise_id = $2",
    )
    .bind(id)
    .bind(enterprise_id)
    .execute(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "deleted": true }))))
}

async fn export_csv(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT p.stage, u.username, u.display_name, u.skill_domain, u.title,
               u.total_fragments, p.salary_proposed_eur, p.notes, p.last_action_at, p.created_at
        FROM enterprise_pipeline_entries p
        JOIN users u ON u.id = p.talent_id
        WHERE p.enterprise_id = $1
        ORDER BY p.stage, p.last_action_at DESC
        "#,
    )
    .bind(enterprise_id)
    .fetch_all(&state.db)
    .await?;
    use sqlx::Row;
    let mut csv = String::from("stage;username;display_name;skill_domain;title;total_fragments;salary_proposed_eur;notes;last_action_at;created_at\n");
    for r in &rows {
        let line = format!(
            "{};{};{};{};{};{};{};{};{};{}\n",
            r.get::<String, _>("stage"),
            r.get::<String, _>("username"),
            r.get::<String, _>("display_name").replace(';', ","),
            r.get::<String, _>("skill_domain"),
            r.get::<String, _>("title"),
            r.get::<i32, _>("total_fragments"),
            r.get::<Option<i32>, _>("salary_proposed_eur").map(|v| v.to_string()).unwrap_or_default(),
            r.get::<Option<String>, _>("notes").unwrap_or_default().replace(';', ",").replace('\n', " "),
            r.get::<chrono::DateTime<chrono::Utc>, _>("last_action_at").format("%Y-%m-%dT%H:%M:%S"),
            r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").format("%Y-%m-%dT%H:%M:%S"),
        );
        csv.push_str(&line);
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"skilluv-pipeline.csv\"".to_string(),
            ),
        ],
        csv,
    ))
}
