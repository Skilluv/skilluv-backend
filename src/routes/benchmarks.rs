//! Measured claims, and the reviewer who re-ran them.
//!
//! Migration 0182 gave benchmarks a table and 0197 made it domain-agnostic.
//! Neither gave anybody a way to write a row, which left the whole design
//! inert: a benchmark's value is that a second person ran it, and there was
//! no endpoint for that second person to say so.
//!
//! ## Who may do what
//!
//! Recording is for whoever produced the work. Reproducing is for a reviewer
//! of that trade — and never for the author. Somebody confirming their own
//! measurement is the exact thing a reproduction is supposed to rule out, and
//! the check is in the handler rather than in the documentation.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn benchmark_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/slices/{slice_id}/benchmarks",
            get(list_benchmarks).post(record_benchmark),
        )
        .route("/benchmarks/{id}/reproduce", post(reproduce_benchmark))
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct BenchmarkRow {
    pub id: Uuid,
    pub slice_id: Uuid,
    pub benchmark_name: String,
    pub metric_name: String,
    pub metric_unit: String,
    pub metric_value: f64,
    /// Latency and throughput move in opposite directions; the metric name
    /// alone does not say which way is better.
    pub lower_is_better: bool,
    #[schema(value_type = Object)]
    pub comparison_baselines: serde_json::Value,
    pub methodology_md: String,
    pub harness: Option<String>,
    pub code_url: String,
    pub dataset_url: Option<String>,
    pub dataset_split: Option<String>,
    pub reproduced_at: Option<chrono::DateTime<chrono::Utc>>,
    pub reproduction_notes: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordBenchmarkBody {
    #[schema(max_length = 120)]
    pub benchmark_name: String,
    #[schema(max_length = 60)]
    pub metric_name: String,
    #[schema(max_length = 20)]
    pub metric_unit: String,
    pub metric_value: f64,
    pub lower_is_better: bool,
    /// `[{"name": "...", "value": 1.0}]`. At least one, or the claim has no
    /// second term.
    #[schema(value_type = Object)]
    pub comparison_baselines: serde_json::Value,
    /// Hardware, input size, warm-up, iterations. Refused under forty
    /// characters: its absence is what makes a benchmark unfalsifiable.
    pub methodology_md: String,
    #[schema(max_length = 40)]
    pub harness: Option<String>,
    pub code_url: String,
    /// Which dataset and split, for an evaluation score. MMLU on the full
    /// test set and MMLU on a sample are different claims.
    pub dataset_url: Option<String>,
    #[schema(max_length = 60)]
    pub dataset_split: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReproduceBody {
    /// What the reviewer got, and on what. Optional, because "same numbers"
    /// is a complete answer.
    pub notes: Option<String>,
}

/// Everything measured on one slice.
#[utoipa::path(
    get, path = "/api/slices/{slice_id}/benchmarks", tag = "slices",
    params(("slice_id" = Uuid, Path, description = "Slice id")),
    responses(
        (status = 200, description = "Benchmarks on this slice", body = ApiResponse<Vec<BenchmarkRow>>),
    ),
)]
pub async fn list_benchmarks(
    State(state): State<AppState>,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<BenchmarkRow>>>, AppError> {
    let rows = sqlx::query_as::<_, BenchmarkRow>(
        r#"
        SELECT id, slice_id, benchmark_name, metric_name, metric_unit,
               metric_value, lower_is_better, comparison_baselines,
               methodology_md, harness, code_url, dataset_url, dataset_split,
               reproduced_at, reproduction_notes
          FROM benchmark_results
         WHERE slice_id = $1
         ORDER BY reproduced_at DESC NULLS LAST, created_at DESC
        "#,
    )
    .bind(slice_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}

/// Record a measurement on a slice you worked on.
///
/// The database refuses a benchmark with no baseline, no method or no code,
/// so this handler does not restate those rules — it checks the one thing SQL
/// cannot: that the caller is the person whose work this is.
#[utoipa::path(
    post, path = "/api/slices/{slice_id}/benchmarks", tag = "slices",
    params(("slice_id" = Uuid, Path, description = "Slice id")),
    request_body = RecordBenchmarkBody,
    responses(
        (status = 200, description = "Recorded", body = ApiResponse<BenchmarkRow>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not your work", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_benchmark(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
    Json(body): Json<RecordBenchmarkBody>,
) -> Result<Json<ApiResponse<BenchmarkRow>>, AppError> {
    require_worked_on(&state, auth.user_id, slice_id).await?;

    let row = sqlx::query_as::<_, BenchmarkRow>(
        r#"
        INSERT INTO benchmark_results
            (slice_id, benchmark_name, metric_name, metric_unit, metric_value,
             lower_is_better, comparison_baselines, methodology_md, harness,
             code_url, dataset_url, dataset_split)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        RETURNING id, slice_id, benchmark_name, metric_name, metric_unit,
                  metric_value, lower_is_better, comparison_baselines,
                  methodology_md, harness, code_url, dataset_url, dataset_split,
                  reproduced_at, reproduction_notes
        "#,
    )
    .bind(slice_id)
    .bind(&body.benchmark_name)
    .bind(&body.metric_name)
    .bind(&body.metric_unit)
    .bind(body.metric_value)
    .bind(body.lower_is_better)
    .bind(&body.comparison_baselines)
    .bind(&body.methodology_md)
    .bind(body.harness.as_deref())
    .bind(&body.code_url)
    .bind(body.dataset_url.as_deref())
    .bind(body.dataset_split.as_deref())
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(row)))
}

/// Confirm you re-ran somebody else's benchmark and got comparable numbers.
///
/// This is the event the whole table exists for: an unreproduced measurement
/// scores nothing and attests nothing.
#[utoipa::path(
    post, path = "/api/benchmarks/{id}/reproduce", tag = "slices",
    params(("id" = Uuid, Path, description = "Benchmark id")),
    request_body = ReproduceBody,
    responses(
        (status = 200, description = "Reproduction recorded", body = ApiResponse<BenchmarkRow>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not a reviewer of this trade, or your own work", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such benchmark", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn reproduce_benchmark(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReproduceBody>,
) -> Result<Json<ApiResponse<BenchmarkRow>>, AppError> {
    let slice_id: Option<Uuid> =
        sqlx::query_scalar("SELECT slice_id FROM benchmark_results WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let Some(slice_id) = slice_id else {
        return Err(AppError::NotFound(format!("benchmark {id} not found")));
    };

    require_reviewer_of_someone_elses_work(&state, auth.user_id, slice_id).await?;

    let row = sqlx::query_as::<_, BenchmarkRow>(
        r#"
        UPDATE benchmark_results
           SET reproduced_at = NOW(),
               reproduced_by_user_id = $2,
               reproduction_notes = $3,
               updated_at = NOW()
         WHERE id = $1
        RETURNING id, slice_id, benchmark_name, metric_name, metric_unit,
                  metric_value, lower_is_better, comparison_baselines,
                  methodology_md, harness, code_url, dataset_url, dataset_split,
                  reproduced_at, reproduction_notes
        "#,
    )
    .bind(id)
    .bind(auth.user_id)
    .bind(body.notes.as_deref())
    .fetch_one(&state.db)
    .await?;

    // A reproduction is what turns a benchmark into an attestation, so the
    // proof engines run now rather than at the next weekly sweep.
    let db = state.db.clone();
    let author = author_of(&state, slice_id).await?;
    if let Some(author) = author {
        tokio::spawn(async move {
            let _ = crate::services::proof_hooks::recompute_all_for_user(&db, author).await;
        });
    }

    Ok(Json(ApiResponse::new(row)))
}

// ═══════════════════════════════════════════════════════════════════
// Shared checks
// ═══════════════════════════════════════════════════════════════════

/// The person whose verified deliverable this slice carries, if any.
pub(crate) async fn author_of(state: &AppState, slice_id: Uuid) -> Result<Option<Uuid>, AppError> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT d.user_id FROM deliverables d
         WHERE d.slice_id = $1 AND d.revoked_at IS NULL
         ORDER BY d.created_at ASC
         LIMIT 1
        "#,
    )
    .bind(slice_id)
    .fetch_optional(&state.db)
    .await?)
}

/// Refuse anybody who did not do the work.
///
/// Either they claimed the slice or they have a deliverable on it. Both,
/// because a slice can be delivered without ever being claimed — an ingested
/// issue somebody solved and submitted — and requiring only the claim would
/// lock those people out of describing their own results.
pub(crate) async fn require_worked_on(
    state: &AppState,
    user_id: Uuid,
    slice_id: Uuid,
) -> Result<(), AppError> {
    let worked: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM project_slices ps
             WHERE ps.id = $1 AND ps.claimed_by_user_id = $2
        ) OR EXISTS (
            SELECT 1 FROM deliverables d
             WHERE d.slice_id = $1 AND d.user_id = $2 AND d.revoked_at IS NULL
        )
        "#,
    )
    .bind(slice_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;

    if !worked {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// Refuse anybody who is not a reviewer of this trade, and the author.
///
/// Confirming your own measurement is the exact thing a reproduction exists
/// to rule out, so the author is refused even when they hold review rights.
pub(crate) async fn require_reviewer_of_someone_elses_work(
    state: &AppState,
    user_id: Uuid,
    slice_id: Uuid,
) -> Result<(), AppError> {
    // Said as a rule rather than as a permission: no capability would let
    // somebody confirm their own measurement, so a bare 403 would send them
    // asking for rights that cannot help.
    if author_of(state, slice_id).await? == Some(user_id) {
        return Err(AppError::Validation(
            "a reproduction has to be somebody else's: confirming your own \
             measurement is what it exists to rule out"
                .into(),
        ));
    }

    let orientation_slug: Option<String> = sqlx::query_scalar(
        r#"
        SELECT o.slug FROM project_slices ps
          JOIN orientations o ON o.id = ps.orientation_id
         WHERE ps.id = $1
        "#,
    )
    .bind(slice_id)
    .fetch_optional(&state.db)
    .await?;

    // A slice with no trade has no reviewer group, so nobody can be shown to
    // hold the right competence. Refusing names the fix rather than reading
    // as a permission problem.
    let Some(slug) = orientation_slug else {
        return Err(AppError::Validation(
            "this slice belongs to no trade, so review rights cannot be \
             checked for it — set its orientation first"
                .into(),
        ));
    };

    crate::middleware::capabilities::require_reviewer_for_orientation(&state.db, user_id, &slug)
        .await
}
