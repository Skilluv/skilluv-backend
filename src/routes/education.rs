//! Education work: the profile, the cohorts, what came out of them, and who
//! else ran the curriculum.
//!
//! ## What is not here
//!
//! The toolkit, the onboarding guides, the brief templates and the writeup
//! templates. They are `content_guides` rows and
//! `/api/guides?domain=education` already serves them.
//!
//! The challenge catalogue, the contests, the missions and the badges are the
//! platform's own, keyed by domain. Education adds rows to them and no
//! routes. The revision rounds are `routes::slice_revisions` and the external
//! accounts are `routes::portfolios`, both of which serve every domain.
//!
//! Creating and joining a cohort is `routes::cohorts`, which has existed
//! since migration 0221 and needed one thing to serve a taught cohort: a
//! teacher. That is a column, not an endpoint.
//!
//! ## What is genuinely specific to this domain
//!
//! Learners. Every artefact here is about real people who are not members,
//! are sometimes minors, and never asked to be evidence in somebody's
//! portfolio — so the endpoints that touch them are the ones with the
//! strictest rules on the platform:
//!
//!   * an outcome row is written by the teacher and readable in full by the
//!     teacher and the learner it is about, and by nobody else;
//!   * a testimonial without consent cannot be stored at all, which the
//!     schema enforces rather than this module;
//!   * what a public profile shows is aggregate, and no endpoint here returns
//!     a learner list to anybody but the teacher.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::education_profile;

pub fn education_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/users/{username}/education-profile",
            get(user_education_profile),
        )
        .route(
            "/education/cohorts/{cohort_id}/outcomes",
            get(list_outcomes).put(record_outcome),
        )
        .route(
            "/education/cohorts/{cohort_id}/conclude",
            axum::routing::post(conclude_cohort),
        )
        .route(
            "/education/slices/{slice_id}/learner-data-cleared",
            axum::routing::post(declare_learner_data_cleared),
        )
        .route(
            "/education/curriculums/{slice_id}/adoptions",
            get(list_adoptions).post(adopt_curriculum),
        )
}

// ═══════════════════════════════════════════════════════════════════
// GET /users/{username}/education-profile
// ═══════════════════════════════════════════════════════════════════

/// Everything one person has to show in the education trades.
///
/// Aggregate only where learners are concerned: a cohort appears with its
/// headcount and its completion figure, and never with a name.
#[utoipa::path(
    get, path = "/api/users/{username}/education-profile", tag = "education",
    params(("username" = String, Path, description = "Public username")),
    responses(
        (status = 200, description = "Profile", body = ApiResponse<education_profile::EducationProfile>),
        (status = 404, description = "No such user", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn user_education_profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<education_profile::EducationProfile>>, AppError> {
    let profile = education_profile::build(&state.db, &username).await?;
    Ok(Json(ApiResponse::new(profile)))
}

// ═══════════════════════════════════════════════════════════════════
// Cohort outcomes
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct OutcomeRow {
    pub learner_user_id: Uuid,
    pub learner_username: String,
    pub pre_assessment: serde_json::Value,
    pub post_assessment: serde_json::Value,
    /// Read from `cohort_members.graduated_at`, not from this table.
    /// Finishing is a fact about somebody's participation, and the outcome
    /// row is about what changed for them — migration 0532 separated the two
    /// when both domains that run cohorts turned out to record it twice.
    pub completed: bool,
    pub satisfaction: Option<i16>,
    pub testimonial_md: String,
    pub testimonial_consent_at: Option<chrono::DateTime<chrono::Utc>>,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

/// What was recorded for each learner in a cohort.
///
/// The teacher sees the whole cohort. A learner sees their own row and
/// nothing else. Everybody else gets a 403, including a curator: an outcome
/// row is a fact about somebody's difficulty learning, and the platform's
/// interest in moderating does not extend to reading it.
#[utoipa::path(
    get, path = "/api/education/cohorts/{cohort_id}/outcomes", tag = "education",
    params(("cohort_id" = Uuid, Path, description = "Cohort")),
    responses(
        (status = 200, description = "Outcomes", body = ApiResponse<Vec<OutcomeRow>>),
        (status = 403, description = "Neither the teacher nor the learner", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_outcomes(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(cohort_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<OutcomeRow>>>, AppError> {
    let teaches = leads_cohort(&state.db, auth.user_id, cohort_id).await?;

    let rows: Vec<OutcomeRow> = sqlx::query_as(
        r#"
        SELECT o.learner_user_id, u.username AS learner_username,
               o.pre_assessment, o.post_assessment,
               (m.graduated_at IS NOT NULL) AS completed,
               o.satisfaction, o.testimonial_md, o.testimonial_consent_at,
               o.recorded_at
          FROM education_learner_outcomes o
          JOIN users u ON u.id = o.learner_user_id
          LEFT JOIN cohort_members m
                 ON m.cohort_id = o.cohort_id AND m.user_id = o.learner_user_id
         WHERE o.cohort_id = $1
           AND ($2::BOOLEAN OR o.learner_user_id = $3)
         ORDER BY u.username
        "#,
    )
    .bind(cohort_id)
    .bind(teaches)
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    // A person who neither taught it nor is in it gets nothing rather than an
    // empty list: the difference tells them whether the cohort exists.
    if !teaches && rows.is_empty() {
        return Err(AppError::Forbidden);
    }

    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordOutcomeBody {
    pub learner_user_id: Uuid,
    /// What was assessed at the start. Free-form: what is assessed differs
    /// per programme.
    pub pre_assessment: Option<serde_json::Value>,
    pub post_assessment: Option<serde_json::Value>,
    /// Whether they finished. Written to `cohort_members.graduated_at`
    /// rather than to the outcome row: one model for both domains that run
    /// cohorts, so "how many finished" has one answer.
    ///
    /// `false` clears a graduation rather than doing nothing, because a
    /// teacher who marked the wrong person needs a way back.
    pub completed: Option<bool>,
    /// One to five. A signal about whether people come back, never evidence
    /// that anybody learned.
    pub satisfaction: Option<i16>,
}

/// Record what happened to one learner.
///
/// Written by the teacher, because they are the one who knows. Not the
/// testimonial: that is the learner's own text and does not arrive through
/// this endpoint.
#[utoipa::path(
    put, path = "/api/education/cohorts/{cohort_id}/outcomes", tag = "education",
    params(("cohort_id" = Uuid, Path, description = "Cohort")),
    request_body = RecordOutcomeBody,
    responses(
        (status = 200, description = "Recorded", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Not a member of that cohort", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not the teacher", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_outcome(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(cohort_id): Path<Uuid>,
    Json(body): Json<RecordOutcomeBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    if !leads_cohort(&state.db, auth.user_id, cohort_id).await? {
        return Err(AppError::Forbidden);
    }
    if body.satisfaction.is_some_and(|s| !(1..=5).contains(&s)) {
        return Err(AppError::Validation("satisfaction is one to five".into()));
    }

    // The learner has to be in the cohort. Without the check an outcome could
    // be recorded against anybody, and the completion rate that gates the
    // attestation would be a number the teacher chose.
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM cohort_members
                         WHERE cohort_id = $1 AND user_id = $2)",
    )
    .bind(cohort_id)
    .bind(body.learner_user_id)
    .fetch_one(&state.db)
    .await?;

    if !is_member {
        return Err(AppError::Validation(
            "that person is not a member of this cohort".into(),
        ));
    }

    sqlx::query(
        r#"
        INSERT INTO education_learner_outcomes
            (cohort_id, learner_user_id, pre_assessment, post_assessment,
             satisfaction, recorded_by)
        VALUES ($1, $2, COALESCE($3, '{}'::JSONB), COALESCE($4, '{}'::JSONB),
                $5, $6)
        ON CONFLICT (cohort_id, learner_user_id) DO UPDATE SET
            pre_assessment  = COALESCE($3, education_learner_outcomes.pre_assessment),
            post_assessment = COALESCE($4, education_learner_outcomes.post_assessment),
            satisfaction    = COALESCE($5, education_learner_outcomes.satisfaction),
            recorded_by     = $6,
            updated_at      = NOW()
        "#,
    )
    .bind(cohort_id)
    .bind(body.learner_user_id)
    .bind(body.pre_assessment)
    .bind(body.post_assessment)
    .bind(body.satisfaction)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    // Completion goes on the membership. Guarded on `left_at IS NULL` for the
    // constraint 0462 added: somebody cannot have both graduated and left, and
    // a departure carries a reason, which is a stronger record than a boolean.
    if let Some(completed) = body.completed {
        let updated = sqlx::query(
            r#"
            UPDATE cohort_members
               SET graduated_at = CASE WHEN $3 THEN COALESCE(graduated_at, NOW())
                                       ELSE NULL END
             WHERE cohort_id = $1 AND user_id = $2
               AND ($3 IS FALSE OR left_at IS NULL)
            "#,
        )
        .bind(cohort_id)
        .bind(body.learner_user_id)
        .bind(completed)
        .execute(&state.db)
        .await?;

        if updated.rows_affected() == 0 && completed {
            return Err(AppError::Validation(
                "that person left this cohort — record the departure differently \
                 before marking them as having finished"
                    .into(),
            ));
        }
    }

    Ok(Json(ApiResponse::new(json!({ "recorded": true }))))
}

/// Close a cohort.
///
/// The moment the attestation generator waits for. A cohort past its end date
/// that nobody concluded is one that fell apart, and that is worth being able
/// to tell apart from one that ran to the end — so concluding is an act
/// rather than a date passing.
#[utoipa::path(
    post, path = "/api/education/cohorts/{cohort_id}/conclude",
    operation_id = "educationConcludeCohort",
    tag = "education",
    params(("cohort_id" = Uuid, Path, description = "Cohort")),
    responses(
        (status = 200, description = "Concluded", body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Not the teacher", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn conclude_cohort(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(cohort_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let done = sqlx::query(
        "UPDATE cohorts SET concluded_at = NOW(), updated_at = NOW()
          WHERE id = $1 AND led_by_user_id = $2 AND concluded_at IS NULL",
    )
    .bind(cohort_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::Forbidden);
    }

    // What the teacher wants to know next: whether the cohort as recorded
    // supports the claim its report will make.
    let attestable: bool = sqlx::query_scalar("SELECT education_cohort_meets_threshold($1)")
        .bind(cohort_id)
        .fetch_one(&state.db)
        .await?;

    Ok(Json(ApiResponse::new(json!({
        "concluded": true,
        // Said out loud rather than left to be discovered when no attestation
        // appears: the usual reason is that outcomes were never recorded.
        "meets_attestation_threshold": attestable,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// The learner-data declaration
// ═══════════════════════════════════════════════════════════════════

/// State that no identifiable learner remains in this artefact.
///
/// This is the statement the attestation generators read — not a row count,
/// because a report with no names and a declaration and a report nobody
/// looked at have the same row count, and those two must not read the same to
/// something about to publish.
///
/// It is signed: the row records who said it and when, and it can be
/// withdrawn if it turns out to be wrong.
#[utoipa::path(
    post, path = "/api/education/slices/{slice_id}/learner-data-cleared", tag = "education",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    responses(
        (status = 200, description = "Declared", body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Not the author of this delivery", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such education artefact", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn declare_learner_data_cleared(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let exists: Option<bool> = sqlx::query_scalar(
        "SELECT slice_type = 'education_artifact' FROM project_slices WHERE id = $1",
    )
    .bind(slice_id)
    .fetch_optional(&state.db)
    .await?;

    match exists {
        None | Some(false) => {
            return Err(AppError::NotFound("no such education artefact".into()));
        }
        Some(true) => {}
    }

    // The author, or somebody holding review rights over the domain. Not the
    // project steward: a steward decides what work exists, and this is a
    // statement about what a delivery contains.
    let is_author: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM deliverables
                         WHERE slice_id = $1 AND user_id = $2)",
    )
    .bind(slice_id)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    if !is_author {
        crate::middleware::capabilities::require_any_capability(
            &state.db,
            auth.user_id,
            &["education_reviewer:all", "admin"],
        )
        .await?;
    }

    sqlx::query(
        "UPDATE project_slices
            SET education_learner_data_cleared_at = NOW(),
                education_learner_data_cleared_by = $2,
                updated_at = NOW()
          WHERE id = $1",
    )
    .bind(slice_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({ "declared": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// Curriculum adoption
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct AdoptionRow {
    pub adopter_username: String,
    pub cohort_id: Option<Uuid>,
    pub feedback_md: String,
    pub adopted_at: chrono::DateTime<chrono::Utc>,
}

/// Who has run this curriculum.
///
/// Public: it is the evidence behind `education_curriculum_authored`, and an
/// attestation whose evidence needs an account to read is one nobody outside
/// can check.
#[utoipa::path(
    get, path = "/api/education/curriculums/{slice_id}/adoptions", tag = "education",
    params(("slice_id" = Uuid, Path, description = "Curriculum slice")),
    responses((status = 200, description = "Adoptions", body = ApiResponse<Vec<AdoptionRow>>)),
)]
pub async fn list_adoptions(
    State(state): State<AppState>,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<AdoptionRow>>>, AppError> {
    let rows: Vec<AdoptionRow> = sqlx::query_as(
        "SELECT u.username AS adopter_username, a.cohort_id, a.feedback_md, a.adopted_at
           FROM education_curriculum_adoptions a
           JOIN users u ON u.id = a.adopter_user_id
          WHERE a.curriculum_slice_id = $1
          ORDER BY a.adopted_at",
    )
    .bind(slice_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdoptBody {
    /// The cohort it was run for, when that was here. Absent when it was run
    /// elsewhere, which is the common case and still worth recording.
    pub cohort_id: Option<Uuid>,
    /// What you changed and what you would change. The part the author reads.
    pub feedback_md: Option<String>,
}

/// Say that you have run somebody else's curriculum.
///
/// The fact `education_curriculum_authored` rests on. Not the author's to
/// claim: the trigger of migration 0524 refuses an adoption by the person who
/// wrote it, because otherwise every curriculum would be adopted once on the
/// day it was published.
#[utoipa::path(
    post, path = "/api/education/curriculums/{slice_id}/adoptions", tag = "education",
    params(("slice_id" = Uuid, Path, description = "Curriculum slice")),
    request_body = AdoptBody,
    responses(
        (status = 200, description = "Recorded", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Not a curriculum, or your own", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn adopt_curriculum(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
    Json(body): Json<AdoptBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let feedback = body.feedback_md.as_deref().unwrap_or("");
    crate::validators::check_max_len(feedback, "feedback_md", 8000)?;

    let subtype: Option<String> =
        sqlx::query_scalar("SELECT education_subtype FROM project_slices WHERE id = $1")
            .bind(slice_id)
            .fetch_optional(&state.db)
            .await?
            .flatten();

    if subtype.as_deref() != Some("curriculum_document") {
        return Err(AppError::Validation(
            "that artefact is not a curriculum".into(),
        ));
    }

    // A cohort named here has to be one the caller led: claiming to have run
    // somebody else's curriculum for a cohort you did not teach is two false
    // statements at once.
    if let Some(cohort_id) = body.cohort_id
        && !leads_cohort(&state.db, auth.user_id, cohort_id).await?
    {
        return Err(AppError::Validation("you did not lead that cohort".into()));
    }

    // The trigger of 0524 refuses the author. Its message is the one worth
    // showing, so it is turned into a validation error rather than a 500.
    let inserted = sqlx::query(
        "INSERT INTO education_curriculum_adoptions
             (curriculum_slice_id, adopter_user_id, cohort_id, feedback_md)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (curriculum_slice_id, adopter_user_id) DO UPDATE
             SET cohort_id = EXCLUDED.cohort_id,
                 feedback_md = EXCLUDED.feedback_md",
    )
    .bind(slice_id)
    .bind(auth.user_id)
    .bind(body.cohort_id)
    .bind(feedback.trim())
    .execute(&state.db)
    .await;

    if let Err(e) = inserted {
        if e.as_database_error().is_some_and(|db| {
            db.message()
                .contains("not adopted by the person who wrote it")
        }) {
            return Err(AppError::Validation(
                "a curriculum is not adopted by the person who wrote it".into(),
            ));
        }
        return Err(e.into());
    }

    Ok(Json(ApiResponse::new(json!({ "adopted": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// Mentors
// ═══════════════════════════════════════════════════════════════════

/// Whether this person is the teacher of this cohort.
async fn leads_cohort(db: &sqlx::PgPool, user_id: Uuid, cohort_id: Uuid) -> Result<bool, AppError> {
    Ok(
        sqlx::query_scalar::<_, bool>("SELECT led_by_user_id = $2 FROM cohorts WHERE id = $1")
            .bind(cohort_id)
            .bind(user_id)
            .fetch_optional(db)
            .await?
            .unwrap_or(false),
    )
}
