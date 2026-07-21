//! Phase 3.4 — profile enrichment endpoints (experiences, educations, languages, availability).

use axum::extract::{Path, State};
use axum::routing::{delete, get, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn profile_extras_routes() -> Router<AppState> {
    Router::new()
        // Availability + salary
        .route(
            "/profile/me/availability",
            get(get_availability).put(update_availability),
        )
        // Experiences
        .route(
            "/profile/me/experiences",
            get(list_experiences).post(add_experience),
        )
        .route(
            "/profile/me/experiences/{id}",
            put(update_experience).delete(delete_experience),
        )
        // Education
        .route(
            "/profile/me/educations",
            get(list_educations).post(add_education),
        )
        .route(
            "/profile/me/educations/{id}",
            put(update_education).delete(delete_education),
        )
        // Languages
        .route(
            "/profile/me/languages",
            get(list_languages)
                .put(set_language)
                .delete(clear_languages),
        )
        .route("/profile/me/languages/{code}", delete(remove_language))
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

// ─── Availability + salary ───────────────────────────────────────

async fn get_availability(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let row: (Option<bool>, Option<String>, Option<i32>, Option<i32>, String) = sqlx::query_as(
        "SELECT available_for_hire, looking_for, salary_range_min_eur, salary_range_max_eur, salary_visibility FROM users WHERE id = $1",
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(build_response(json!({
        "available_for_hire": row.0.unwrap_or(false),
        "looking_for": row.1,
        "salary_range_min_eur": row.2,
        "salary_range_max_eur": row.3,
        "salary_visibility": row.4,
    }))))
}

#[derive(Deserialize)]
struct AvailabilityBody {
    available_for_hire: Option<bool>,
    looking_for: Option<String>,
    salary_range_min_eur: Option<i32>,
    salary_range_max_eur: Option<i32>,
    salary_visibility: Option<String>,
}

async fn update_availability(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<AvailabilityBody>,
) -> Result<Json<Value>, AppError> {
    if let Some(ref lf) = body.looking_for
        && !matches!(
            lf.as_str(),
            "cdi" | "cdd" | "freelance" | "internship" | "contract"
        )
    {
        return Err(AppError::Validation("invalid looking_for".into()));
    }
    if let Some(ref vis) = body.salary_visibility
        && !matches!(vis.as_str(), "private" | "enterprise_only" | "public")
    {
        return Err(AppError::Validation("invalid salary_visibility".into()));
    }
    if let (Some(min), Some(max)) = (body.salary_range_min_eur, body.salary_range_max_eur)
        && min > max
    {
        return Err(AppError::Validation("salary min cannot exceed max".into()));
    }
    sqlx::query(
        r#"
        UPDATE users SET
            available_for_hire = COALESCE($1, available_for_hire),
            looking_for = COALESCE($2, looking_for),
            salary_range_min_eur = COALESCE($3, salary_range_min_eur),
            salary_range_max_eur = COALESCE($4, salary_range_max_eur),
            salary_visibility = COALESCE($5, salary_visibility),
            updated_at = NOW()
        WHERE id = $6
        "#,
    )
    .bind(body.available_for_hire)
    .bind(body.looking_for.as_deref())
    .bind(body.salary_range_min_eur)
    .bind(body.salary_range_max_eur)
    .bind(body.salary_visibility.as_deref())
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "updated": true }))))
}

// ─── Experiences ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct ExperienceInput {
    company: String,
    title: String,
    description: Option<String>,
    started_on: chrono::NaiveDate,
    ended_on: Option<chrono::NaiveDate>,
    position: Option<i32>,
}

async fn list_experiences(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        "SELECT id, company, title, description, started_on, ended_on, position FROM user_experiences WHERE user_id = $1 ORDER BY started_on DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    use sqlx::Row;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "company": r.get::<String, _>("company"),
                "title": r.get::<String, _>("title"),
                "description": r.get::<Option<String>, _>("description"),
                "started_on": r.get::<chrono::NaiveDate, _>("started_on"),
                "ended_on": r.get::<Option<chrono::NaiveDate>, _>("ended_on"),
                "position": r.get::<i32, _>("position"),
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "experiences": items }))))
}

async fn add_experience(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ExperienceInput>,
) -> Result<Json<Value>, AppError> {
    if body.company.trim().is_empty() || body.title.trim().is_empty() {
        return Err(AppError::Validation("company and title required".into()));
    }
    let id: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO user_experiences (user_id, company, title, description, started_on, ended_on, position)
        VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id
        "#,
    )
    .bind(auth.user_id)
    .bind(body.company.trim())
    .bind(body.title.trim())
    .bind(body.description.as_deref().map(str::trim))
    .bind(body.started_on)
    .bind(body.ended_on)
    .bind(body.position.unwrap_or(0))
    .fetch_one(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "id": id.0 }))))
}

async fn update_experience(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ExperienceInput>,
) -> Result<Json<Value>, AppError> {
    sqlx::query(
        r#"
        UPDATE user_experiences
        SET company = $1, title = $2, description = $3, started_on = $4, ended_on = $5, position = $6
        WHERE id = $7 AND user_id = $8
        "#,
    )
    .bind(body.company.trim())
    .bind(body.title.trim())
    .bind(body.description.as_deref().map(str::trim))
    .bind(body.started_on)
    .bind(body.ended_on)
    .bind(body.position.unwrap_or(0))
    .bind(id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "updated": true }))))
}

async fn delete_experience(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    sqlx::query("DELETE FROM user_experiences WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(build_response(json!({ "deleted": true }))))
}

// ─── Education ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct EducationInput {
    school: String,
    degree: Option<String>,
    field: Option<String>,
    started_on: chrono::NaiveDate,
    ended_on: Option<chrono::NaiveDate>,
    position: Option<i32>,
}

async fn list_educations(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        "SELECT id, school, degree, field, started_on, ended_on, position FROM user_educations WHERE user_id = $1 ORDER BY started_on DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    use sqlx::Row;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "school": r.get::<String, _>("school"),
                "degree": r.get::<Option<String>, _>("degree"),
                "field": r.get::<Option<String>, _>("field"),
                "started_on": r.get::<chrono::NaiveDate, _>("started_on"),
                "ended_on": r.get::<Option<chrono::NaiveDate>, _>("ended_on"),
                "position": r.get::<i32, _>("position"),
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "educations": items }))))
}

async fn add_education(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<EducationInput>,
) -> Result<Json<Value>, AppError> {
    if body.school.trim().is_empty() {
        return Err(AppError::Validation("school required".into()));
    }
    let id: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO user_educations (user_id, school, degree, field, started_on, ended_on, position)
        VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id
        "#,
    )
    .bind(auth.user_id)
    .bind(body.school.trim())
    .bind(body.degree.as_deref().map(str::trim))
    .bind(body.field.as_deref().map(str::trim))
    .bind(body.started_on)
    .bind(body.ended_on)
    .bind(body.position.unwrap_or(0))
    .fetch_one(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "id": id.0 }))))
}

async fn update_education(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<EducationInput>,
) -> Result<Json<Value>, AppError> {
    sqlx::query(
        "UPDATE user_educations SET school = $1, degree = $2, field = $3, started_on = $4, ended_on = $5, position = $6 WHERE id = $7 AND user_id = $8",
    )
    .bind(body.school.trim())
    .bind(body.degree.as_deref().map(str::trim))
    .bind(body.field.as_deref().map(str::trim))
    .bind(body.started_on)
    .bind(body.ended_on)
    .bind(body.position.unwrap_or(0))
    .bind(id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "updated": true }))))
}

async fn delete_education(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    sqlx::query("DELETE FROM user_educations WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(build_response(json!({ "deleted": true }))))
}

// ─── Languages ───────────────────────────────────────────────────

async fn list_languages(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT language, proficiency FROM user_languages WHERE user_id = $1 ORDER BY language",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|(lang, p)| json!({ "language": lang, "proficiency": p }))
        .collect();
    Ok(Json(build_response(json!({ "languages": items }))))
}

#[derive(Deserialize)]
struct LanguageBody {
    language: String,
    proficiency: String,
}

async fn set_language(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<LanguageBody>,
) -> Result<Json<Value>, AppError> {
    let lang = body.language.trim().to_lowercase();
    if lang.len() != 2 || !lang.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(AppError::Validation(
            "language must be a 2-letter ISO 639-1 code".into(),
        ));
    }
    if !matches!(
        body.proficiency.as_str(),
        "A1" | "A2" | "B1" | "B2" | "C1" | "C2" | "native"
    ) {
        return Err(AppError::Validation("invalid proficiency".into()));
    }
    sqlx::query(
        r#"
        INSERT INTO user_languages (user_id, language, proficiency)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, language) DO UPDATE SET proficiency = EXCLUDED.proficiency
        "#,
    )
    .bind(auth.user_id)
    .bind(&lang)
    .bind(&body.proficiency)
    .execute(&state.db)
    .await?;
    Ok(Json(build_response(
        json!({ "language": lang, "proficiency": body.proficiency }),
    )))
}

async fn remove_language(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(code): Path<String>,
) -> Result<Json<Value>, AppError> {
    sqlx::query("DELETE FROM user_languages WHERE user_id = $1 AND language = $2")
        .bind(auth.user_id)
        .bind(code.to_lowercase())
        .execute(&state.db)
        .await?;
    Ok(Json(build_response(json!({ "deleted": true }))))
}

async fn clear_languages(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    sqlx::query("DELETE FROM user_languages WHERE user_id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(build_response(json!({ "cleared": true }))))
}
