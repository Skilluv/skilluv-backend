//! Audio work: the files, the sources they came from, the castings, and the
//! revision rounds.
//!
//! ## What is not here
//!
//! The toolkit, the onboarding guides, the brief templates and the writeup
//! templates. They are `content_guides` rows and `/api/guides?domain=audio`
//! already serves them — a second endpoint would be the mistake
//! `routes::guides` was written to undo.
//!
//! The challenge catalogue, the contests, the missions and the badges are the
//! platform's own, keyed by domain. Audio adds rows to them and no routes.
//!
//! ## Listening is always a signed, short-lived URL
//!
//! Nothing here returns a stable link to audio. Unreleased work for a paying
//! client is the normal case in this domain, and a URL that outlives the
//! request that asked for it is a URL that outlives the embargo.

use axum::extract::{Multipart, Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{audio_attestations, audio_files, audio_profile};

/// The largest single file this endpoint accepts, in bytes.
///
/// Two gigabytes, which is the adaptive-music budget of migration 0509 —
/// there is no point accepting a file no delivery may keep. The per-delivery
/// budget is the one that actually decides, and it is a row.
const MAX_UPLOAD_BYTES: usize = 2 * 1024 * 1024 * 1024;

pub fn audio_routes() -> Router<AppState> {
    Router::new()
        .route("/users/{username}/audio-profile", get(user_audio_profile))
        .route(
            "/audio/slices/{slice_id}/files",
            get(list_files).post(upload_file),
        )
        .route("/audio/files/{file_id}/listen", get(listen))
        .route(
            "/audio/slices/{slice_id}/sources",
            get(list_sources).post(declare_source),
        )
        .route(
            "/audio/slices/{slice_id}/sources/complete",
            post(complete_sources),
        )
        .route(
            "/audio/slices/{slice_id}/revisions",
            get(list_revisions).post(request_revision),
        )
        .route(
            "/audio/revisions/{round_id}/resolve",
            post(resolve_revision),
        )
        .route("/audio/castings", get(list_castings).post(open_casting))
        .route("/audio/castings/{casting_id}", get(get_casting))
        .route("/audio/castings/{casting_id}/auditions", post(audition))
        .route("/audio/castings/{casting_id}/select", post(select_voice))
        .route("/audio/deliverables/{deliverable_id}/credit", post(credit))
        .route("/audio/mentors/for-me", get(mentor_matches))
        .route("/projects/{slug}/credits", get(project_credits))
        .route(
            "/audio/portfolios",
            get(my_portfolios).post(declare_portfolio),
        )
        .route(
            "/audio/portfolios/{id}",
            axum::routing::delete(drop_portfolio),
        )
}

/// Whether this person may act on the work of this slice.
///
/// The slice's own author, or somebody holding review rights over audio. Not
/// the project steward: a steward decides what work exists, and this is about
/// what a delivery contains.
async fn may_edit_slice(
    db: &sqlx::PgPool,
    user_id: Uuid,
    slice_id: Uuid,
) -> Result<bool, AppError> {
    let is_author: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM deliverables
                         WHERE slice_id = $1 AND user_id = $2)",
    )
    .bind(slice_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    if is_author {
        return Ok(true);
    }

    // Review rights over the whole domain, or the platform's own admin. A
    // reviewer of one family cannot edit another family's delivery.
    Ok(crate::middleware::capabilities::require_any_capability(
        db,
        user_id,
        &["audio_reviewer:all", "admin"],
    )
    .await
    .is_ok())
}

async fn require_slice_access(
    db: &sqlx::PgPool,
    user_id: Uuid,
    slice_id: Uuid,
) -> Result<(), AppError> {
    if may_edit_slice(db, user_id, slice_id).await? {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

// ═══════════════════════════════════════════════════════════════════
// GET /users/{username}/audio-profile
// ═══════════════════════════════════════════════════════════════════

/// Everything one person has to show in the audio trades.
#[utoipa::path(
    get, path = "/api/users/{username}/audio-profile", tag = "audio",
    params(("username" = String, Path, description = "Account name")),
    responses(
        (status = 200, description = "Profile", body = ApiResponse<audio_profile::AudioProfile>),
        (status = 404, description = "No such account", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn user_audio_profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<audio_profile::AudioProfile>>, AppError> {
    Ok(Json(ApiResponse::new(
        audio_profile::build(&state.db, &username).await?,
    )))
}

// ═══════════════════════════════════════════════════════════════════
// Files
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct FileRow {
    pub id: Uuid,
    pub role: String,
    pub original_filename: String,
    pub byte_size: i64,
    pub container: String,
    pub duration_ms: Option<i32>,
    pub sample_rate_hz: Option<i32>,
    pub bit_depth: Option<i16>,
    pub channels: Option<i16>,
    /// Integrated loudness, measured. Absent means not measured, never zero.
    ///
    /// Sent as a number rather than the exact decimal the column holds: two
    /// decimal places of LUFS is well inside what a float represents, and a
    /// client that has to parse a string to draw a meter will get it wrong
    /// once.
    pub loudness_lufs: Option<f64>,
    pub true_peak_dbfs: Option<f64>,
    pub loudness_range_lu: Option<f64>,
    pub analysis_status: String,
    pub analysis_error: Option<String>,
    /// Peaks for drawing, 0..100, four hundred of them. Absent until the
    /// analysis has run, or where ffmpeg is not installed.
    #[schema(value_type = Option<Vec<u8>>)]
    pub waveform_peaks: Option<serde_json::Value>,
}

/// The files of one delivery.
#[utoipa::path(
    get, path = "/api/audio/slices/{slice_id}/files", tag = "audio",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    responses(
        (status = 200, description = "Files", body = ApiResponse<Vec<FileRow>>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_files(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<FileRow>>>, AppError> {
    // Readable by any signed-in account: the measurements are what a reviewer
    // reads, and hiding them would make the review grid unusable. The bytes
    // are a different question, and `listen` answers it.
    let rows: Vec<FileRow> = sqlx::query_as(
        "SELECT id, role, original_filename, byte_size, container, duration_ms,
                sample_rate_hz, bit_depth, channels,
                loudness_lufs::FLOAT8 AS loudness_lufs,
                true_peak_dbfs::FLOAT8 AS true_peak_dbfs,
                loudness_range_lu::FLOAT8 AS loudness_range_lu,
                analysis_status, analysis_error, waveform_peaks
           FROM audio_artifact_files
          WHERE slice_id = $1
          ORDER BY role, sort_order, created_at",
    )
    .bind(slice_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}

/// Add a file to a delivery.
///
/// Multipart, with a `role` field and a `file` part. The budget of the slice's
/// subtype decides what fits, and it is a row rather than a constant here.
#[utoipa::path(
    post, path = "/api/audio/slices/{slice_id}/files", tag = "audio",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    responses(
        (status = 200, description = "Stored", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Rejected", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not your delivery", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn upload_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    require_slice_access(&state.db, auth.user_id, slice_id).await?;

    let mut role = String::from("master");
    let mut filename: Option<String> = None;
    let mut bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("malformed upload: {e}")))?
    {
        match field.name().unwrap_or_default() {
            "role" => {
                role = field
                    .text()
                    .await
                    .map_err(|e| AppError::Validation(format!("role: {e}")))?;
            }
            "file" => {
                filename = field.file_name().map(|f| f.to_string());
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::Validation(format!("file: {e}")))?;
                if data.len() > MAX_UPLOAD_BYTES {
                    return Err(AppError::Validation("that file is too large".into()));
                }
                bytes = Some(data.to_vec());
            }
            _ => {}
        }
    }

    let (Some(filename), Some(bytes)) = (filename, bytes) else {
        return Err(AppError::Validation(
            "the upload needs a `file` part with a filename".into(),
        ));
    };

    let id = audio_files::add_file(
        &state.db,
        &state.storage,
        audio_files::NewFile {
            slice_id,
            role: &role,
            original_filename: &filename,
            bytes: &bytes,
            uploaded_by: auth.user_id,
        },
    )
    .await?;

    Ok(Json(ApiResponse::new(json!({
        "id": id,
        // Said out loud so a client does not poll for numbers that may never
        // arrive: the analysis runs on a sweep and is skipped entirely where
        // ffmpeg is not installed.
        "analysis": "pending"
    }))))
}

/// A short-lived URL for listening to one file.
#[utoipa::path(
    get, path = "/api/audio/files/{file_id}/listen", tag = "audio",
    params(("file_id" = Uuid, Path, description = "File")),
    responses(
        (status = 200, description = "Signed URL", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "No such file", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn listen(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(file_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let url = audio_files::listen_url(&state.db, &state.storage, file_id).await?;
    Ok(Json(ApiResponse::new(json!({
        "url": url,
        "expires_in_seconds": audio_files::LISTEN_URL_TTL_SECONDS,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Sources and licences
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct SourceRow {
    pub id: Uuid,
    pub kind: String,
    pub source_name: String,
    pub source_url: Option<String>,
    pub licence_identifier: Option<String>,
    pub attribution_text: Option<String>,
    pub purchased_from: Option<String>,
    pub permits_commercial_use: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeclareSourceBody {
    /// `original`, `public_domain`, `creative_commons`, `royalty_free`,
    /// `licensed_commercial`, `third_party_work`.
    pub kind: String,
    pub source_name: String,
    pub source_url: Option<String>,
    pub licence_identifier: Option<String>,
    /// Required for a Creative Commons licence: the credit line, verbatim.
    pub attribution_text: Option<String>,
    pub purchased_from: Option<String>,
    pub purchase_price_eur: Option<f64>,
    pub permits_commercial_use: Option<bool>,
}

/// Everything this delivery says it was built from.
#[utoipa::path(
    get, path = "/api/audio/slices/{slice_id}/sources", tag = "audio",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    responses((status = 200, description = "Sources", body = ApiResponse<serde_json::Value>)),
)]
pub async fn list_sources(
    State(state): State<AppState>,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    // Public, and deliberately: the provenance of a published piece is what a
    // stranger has to be able to check for the attestation on it to mean
    // anything.
    let rows: Vec<SourceRow> = sqlx::query_as(
        "SELECT id, kind, source_name, source_url, licence_identifier,
                attribution_text, purchased_from, permits_commercial_use
           FROM audio_source_licences
          WHERE slice_id = $1
          ORDER BY kind, source_name",
    )
    .bind(slice_id)
    .fetch_all(&state.db)
    .await?;

    let declared: Option<Option<chrono::DateTime<chrono::Utc>>> =
        sqlx::query_scalar("SELECT audio_sources_declared_at FROM project_slices WHERE id = $1")
            .bind(slice_id)
            .fetch_optional(&state.db)
            .await?;

    Ok(Json(ApiResponse::new(json!({
        "sources": rows,
        // The difference between "nothing was used" and "nobody filled this
        // in". An empty list with no declaration is the second.
        "declared_complete_at": declared.flatten(),
    }))))
}

/// Add one source to the declaration.
#[utoipa::path(
    post, path = "/api/audio/slices/{slice_id}/sources", tag = "audio",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    request_body = DeclareSourceBody,
    responses(
        (status = 200, description = "Recorded", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Rejected", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn declare_source(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
    Json(body): Json<DeclareSourceBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    require_slice_access(&state.db, auth.user_id, slice_id).await?;
    crate::validators::check_max_len(&body.source_name, "source_name", 200)?;
    if let Some(url) = &body.source_url {
        crate::validators::validate_url(url, "source_url", 500)?;
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO audio_source_licences
            (slice_id, kind, source_name, source_url, licence_identifier,
             attribution_text, purchased_from, purchase_price_eur,
             permits_commercial_use, declared_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
        RETURNING id
        "#,
    )
    .bind(slice_id)
    .bind(&body.kind)
    .bind(body.source_name.trim())
    .bind(&body.source_url)
    .bind(&body.licence_identifier)
    .bind(&body.attribution_text)
    .bind(&body.purchased_from)
    .bind(
        body.purchase_price_eur
            .map(bigdecimal::BigDecimal::try_from)
            .transpose()
            .ok()
            .flatten(),
    )
    .bind(body.permits_commercial_use)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    // Adding a source after saying the list was complete makes it incomplete
    // again. Clearing the declaration is the honest consequence, and it costs
    // one click to redo — where leaving it would mean an attestation resting
    // on a statement that is no longer true.
    sqlx::query(
        "UPDATE project_slices
            SET audio_sources_declared_at = NULL, audio_sources_declared_by = NULL
          WHERE id = $1",
    )
    .bind(slice_id)
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({ "id": id }))))
}

/// State that the source list is complete.
///
/// This is the statement the attestation generators read — not the row count,
/// because a wholly original track has no rows and is not undeclared.
#[utoipa::path(
    post, path = "/api/audio/slices/{slice_id}/sources/complete", tag = "audio",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    responses(
        (status = 200, description = "Declared", body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Not your delivery", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn complete_sources(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    require_slice_access(&state.db, auth.user_id, slice_id).await?;

    let done = sqlx::query(
        "UPDATE project_slices
            SET audio_sources_declared_at = NOW(), audio_sources_declared_by = $2
          WHERE id = $1 AND slice_type = 'audio_artifact'",
    )
    .bind(slice_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("no audio slice with that id".into()));
    }

    // The declaration is usually the last thing missing before an attestation
    // can be issued, so the generator runs now rather than at the next sweep.
    // A failure here is not the caller's problem: the declaration is saved and
    // the hourly pass will pick it up.
    if let Err(e) = audio_attestations::issue_for_slice(&state.db, slice_id).await {
        tracing::warn!(slice = %slice_id, error = %e, "attestation pass after declaration failed");
    }

    Ok(Json(ApiResponse::new(json!({ "declared": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// Revision rounds
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct RevisionRow {
    pub id: Uuid,
    pub round_no: i16,
    pub kind: String,
    pub notes_md: String,
    pub requested_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resolution_note: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestRevisionBody {
    /// One of `revision_round_kinds` for this domain.
    pub kind: String,
    pub notes_md: String,
}

/// The rounds this delivery has been through, and how many remain.
#[utoipa::path(
    get, path = "/api/audio/slices/{slice_id}/revisions", tag = "audio",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    responses((status = 200, description = "Rounds", body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn list_revisions(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let rows: Vec<RevisionRow> = sqlx::query_as(
        "SELECT id, round_no, kind, notes_md, requested_at, resolved_at, resolution_note
           FROM slice_revision_rounds WHERE slice_id = $1 ORDER BY round_no",
    )
    .bind(slice_id)
    .fetch_all(&state.db)
    .await?;

    let allowed: Option<i16> = sqlx::query_scalar(
        "SELECT l.max_rounds FROM project_slices ps
           JOIN revision_round_limits l ON l.skill_domain = ps.primary_domain
          WHERE ps.id = $1",
    )
    .bind(slice_id)
    .fetch_optional(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({
        "rounds": rows,
        "max_rounds": allowed,
        "remaining": allowed.map(|a| (a as i64 - rows.len() as i64).max(0)),
    }))))
}

/// Ask for a change.
///
/// Open to whoever commissioned the work rather than to whoever did it: a
/// round the maker can open is a round the maker can spend.
#[utoipa::path(
    post, path = "/api/audio/slices/{slice_id}/revisions", tag = "audio",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    request_body = RequestRevisionBody,
    responses(
        (status = 200, description = "Opened", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "No rounds left, or unknown kind", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_revision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
    Json(body): Json<RequestRevisionBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    if body.notes_md.trim().is_empty() {
        return Err(AppError::Validation(
            "a round has to say what to change — a rejection with no statement \
             cannot be acted on"
                .into(),
        ));
    }
    crate::validators::check_max_len(&body.notes_md, "notes_md", 8000)?;

    // The database counts the rounds and enforces the limit, because the count
    // is the one both sides quote and a check here would race with itself.
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO slice_revision_rounds
            (slice_id, round_no, kind, requested_by, notes_md)
        VALUES (
            $1,
            (SELECT COALESCE(max(round_no), 0) + 1
               FROM slice_revision_rounds WHERE slice_id = $1),
            $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(slice_id)
    .bind(&body.kind)
    .bind(auth.user_id)
    .bind(body.notes_md.trim())
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({ "id": id }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveRevisionBody {
    pub resolution_note: Option<String>,
}

/// Close a round.
///
/// Only the person who opened it. A counter the maker can run down alone is
/// not a count both sides agree on, which is the only kind worth keeping.
#[utoipa::path(
    post, path = "/api/audio/revisions/{round_id}/resolve", tag = "audio",
    params(("round_id" = Uuid, Path, description = "Round")),
    request_body = ResolveRevisionBody,
    responses(
        (status = 200, description = "Closed", body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Not the person who asked", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn resolve_revision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(round_id): Path<Uuid>,
    Json(body): Json<ResolveRevisionBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let done = sqlx::query(
        "UPDATE slice_revision_rounds
            SET resolved_at = NOW(), resolved_by = $2, resolution_note = $3
          WHERE id = $1 AND requested_by = $2 AND resolved_at IS NULL",
    )
    .bind(round_id)
    .bind(auth.user_id)
    .bind(&body.resolution_note)
    .execute(&state.db)
    .await?;

    if done.rows_affected() == 0 {
        // Closed by whoever opened it, and only once.
        return Err(AppError::Forbidden);
    }
    Ok(Json(ApiResponse::new(json!({ "resolved": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// Voice castings
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct CastingRow {
    pub id: Uuid,
    pub slice_id: Uuid,
    pub character_brief_md: String,
    pub sample_line_text: String,
    pub target_language: String,
    pub max_audition_seconds: i16,
    pub is_blind: bool,
    pub audition_deadline: chrono::DateTime<chrono::Utc>,
    pub status: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct CastingQuery {
    /// BCP-47, so `fr` and `fr-BE` are different answers. An accent is part of
    /// the brief in this trade, not a detail.
    #[param(max_length = 20)]
    pub language: Option<String>,
}

/// Castings still taking auditions.
#[utoipa::path(
    get, path = "/api/audio/castings", tag = "audio",
    params(CastingQuery),
    responses((status = 200, description = "Open castings", body = ApiResponse<Vec<CastingRow>>)),
)]
pub async fn list_castings(
    State(state): State<AppState>,
    Query(q): Query<CastingQuery>,
) -> Result<Json<ApiResponse<Vec<CastingRow>>>, AppError> {
    crate::validators::check_max_len_opt(&q.language, "language", 20)?;

    let rows: Vec<CastingRow> = sqlx::query_as(
        "SELECT id, slice_id, character_brief_md, sample_line_text, target_language,
                max_audition_seconds, is_blind, audition_deadline, status
           FROM voice_castings
          WHERE status = 'open' AND audition_deadline > NOW()
            AND ($1::TEXT IS NULL OR target_language = $1)
          ORDER BY audition_deadline",
    )
    .bind(q.language.as_deref())
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenCastingBody {
    pub slice_id: Uuid,
    pub character_brief_md: String,
    pub sample_line_text: String,
    pub target_language: String,
    pub audition_deadline: chrono::DateTime<chrono::Utc>,
    /// Defaults to 90.
    pub max_audition_seconds: Option<i16>,
    /// Defaults to true. Turning it off is a visible choice.
    pub is_blind: Option<bool>,
}

/// Open a call for a voice.
#[utoipa::path(
    post, path = "/api/audio/castings", tag = "audio",
    request_body = OpenCastingBody,
    responses(
        (status = 200, description = "Opened", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Rejected", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_casting(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<OpenCastingBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    require_slice_access(&state.db, auth.user_id, body.slice_id).await?;
    crate::validators::check_max_len(&body.character_brief_md, "character_brief_md", 8000)?;
    crate::validators::check_max_len(&body.sample_line_text, "sample_line_text", 4000)?;
    crate::validators::check_max_len(&body.target_language, "target_language", 20)?;

    if body.audition_deadline <= chrono::Utc::now() {
        return Err(AppError::Validation(
            "the deadline has to be in the future — a casting that closed before \
             it opened wastes everybody who reads it"
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO voice_castings
            (slice_id, opened_by, character_brief_md, sample_line_text,
             target_language, max_audition_seconds, is_blind, audition_deadline)
        VALUES ($1,$2,$3,$4,$5,COALESCE($6, 90),COALESCE($7, TRUE),$8)
        RETURNING id
        "#,
    )
    .bind(body.slice_id)
    .bind(auth.user_id)
    .bind(body.character_brief_md.trim())
    .bind(body.sample_line_text.trim())
    .bind(body.target_language.trim())
    .bind(body.max_audition_seconds)
    .bind(body.is_blind)
    .bind(body.audition_deadline)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({ "id": id }))))
}

/// One casting, with its takes.
///
/// Names are withheld while the casting is blind and still undecided. Not a
/// display concern: the identities never leave this function, so a client
/// cannot show what it was not sent.
#[utoipa::path(
    get, path = "/api/audio/castings/{casting_id}", tag = "audio",
    params(("casting_id" = Uuid, Path, description = "Casting")),
    responses(
        (status = 200, description = "Casting", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "No such casting", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn get_casting(
    State(state): State<AppState>,
    Path(casting_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let casting: Option<CastingRow> = sqlx::query_as(
        "SELECT id, slice_id, character_brief_md, sample_line_text, target_language,
                max_audition_seconds, is_blind, audition_deadline, status
           FROM voice_castings WHERE id = $1",
    )
    .bind(casting_id)
    .fetch_optional(&state.db)
    .await?;

    let casting = casting.ok_or_else(|| AppError::NotFound("casting not found".into()))?;
    let hide_names = casting.is_blind && casting.status != "selected";

    #[derive(sqlx::FromRow)]
    struct Take {
        id: Uuid,
        username: String,
        notes_md: Option<String>,
        duration_ms: Option<i32>,
        submitted_at: chrono::DateTime<chrono::Utc>,
    }

    let takes: Vec<Take> = sqlx::query_as(
        "SELECT s.id, u.username, s.notes_md, s.duration_ms, s.submitted_at
           FROM voice_audition_submissions s
           JOIN users u ON u.id = s.voice_actor_user_id
          WHERE s.casting_id = $1 AND s.withdrawn_at IS NULL
          ORDER BY s.submitted_at",
    )
    .bind(casting_id)
    .fetch_all(&state.db)
    .await?;

    let auditions: Vec<serde_json::Value> = takes
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            json!({
                "id": t.id,
                // A number rather than a name while the casting is blind. The
                // number is stable within one reading so a listener can refer
                // to "the third take" out loud.
                "voice": if hide_names { json!(format!("voix {}", i + 1)) } else { json!(t.username) },
                "notes_md": t.notes_md,
                "duration_ms": t.duration_ms,
                "submitted_at": t.submitted_at,
            })
        })
        .collect();

    Ok(Json(ApiResponse::new(json!({
        "casting": casting,
        "blind": hide_names,
        "auditions": auditions,
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditionBody {
    /// Where the take is, when the actor hosts it themselves.
    pub audition_url: Option<String>,
    /// The id of a file already uploaded through the platform.
    pub audition_file_id: Option<Uuid>,
    pub notes_md: Option<String>,
}

/// Hand in a take.
///
/// A second take replaces the first. A listener comparing two versions of the
/// same voice is doing the actor no favour, and the actor chose which one to
/// send.
#[utoipa::path(
    post, path = "/api/audio/castings/{casting_id}/auditions", tag = "audio",
    params(("casting_id" = Uuid, Path, description = "Casting")),
    request_body = AuditionBody,
    responses(
        (status = 200, description = "Submitted", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Closed, or nothing to listen to", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn audition(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(casting_id): Path<Uuid>,
    Json(body): Json<AuditionBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    if let Some(url) = &body.audition_url {
        crate::validators::validate_url(url, "audition_url", 500)?;
    }
    if let Some(notes) = &body.notes_md {
        crate::validators::check_max_len(notes, "notes_md", 4000)?;
    }

    let open: Option<bool> = sqlx::query_scalar(
        "SELECT status = 'open' AND audition_deadline > NOW()
           FROM voice_castings WHERE id = $1",
    )
    .bind(casting_id)
    .fetch_optional(&state.db)
    .await?;

    match open {
        None => return Err(AppError::NotFound("casting not found".into())),
        Some(false) => {
            return Err(AppError::Validation("this casting is closed".into()));
        }
        Some(true) => {}
    }

    // A take handed in through the platform is a file we already hold; the
    // storage key comes from there rather than from the caller, so nobody can
    // point an audition at somebody else's master.
    let storage_key: Option<String> = match body.audition_file_id {
        Some(file_id) => {
            sqlx::query_scalar(
                "SELECT storage_key FROM audio_artifact_files
                  WHERE id = $1 AND uploaded_by = $2",
            )
            .bind(file_id)
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?
        }
        None => None,
    };

    if storage_key.is_none() && body.audition_url.is_none() {
        return Err(AppError::Validation(
            "an audition needs something to listen to".into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO voice_audition_submissions
            (casting_id, voice_actor_user_id, audition_storage_key, audition_url, notes_md)
        VALUES ($1,$2,$3,$4,$5)
        ON CONFLICT (casting_id, voice_actor_user_id) WHERE withdrawn_at IS NULL
        DO UPDATE SET audition_storage_key = EXCLUDED.audition_storage_key,
                      audition_url = EXCLUDED.audition_url,
                      notes_md = EXCLUDED.notes_md,
                      submitted_at = NOW()
        RETURNING id
        "#,
    )
    .bind(casting_id)
    .bind(auth.user_id)
    .bind(&storage_key)
    .bind(&body.audition_url)
    .bind(&body.notes_md)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({ "id": id }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectVoiceBody {
    pub submission_id: Uuid,
}

/// Choose a voice.
///
/// Only whoever opened the casting. Selecting also lifts the blind, which is
/// the moment the names become useful: everybody who auditioned deserves to
/// know who got it.
#[utoipa::path(
    post, path = "/api/audio/castings/{casting_id}/select", tag = "audio",
    params(("casting_id" = Uuid, Path, description = "Casting")),
    request_body = SelectVoiceBody,
    responses(
        (status = 200, description = "Chosen", body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Not your casting", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn select_voice(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(casting_id): Path<Uuid>,
    Json(body): Json<SelectVoiceBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let done = sqlx::query(
        r#"
        UPDATE voice_castings
           SET status = 'selected',
               selected_submission_id = $2,
               selected_at = NOW()
         WHERE id = $1 AND opened_by = $3 AND status IN ('open', 'reviewing')
           AND EXISTS (SELECT 1 FROM voice_audition_submissions
                        WHERE id = $2 AND casting_id = $1 AND withdrawn_at IS NULL)
        "#,
    )
    .bind(casting_id)
    .bind(body.submission_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    if done.rows_affected() == 0 {
        // Decided by whoever opened it, from a take that was actually handed in.
        return Err(AppError::Forbidden);
    }

    Ok(Json(ApiResponse::new(json!({ "selected": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// Credits
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreditBody {
    /// Where the credit appears — a store page, an end-titles screenshot, a
    /// podcast description. Stored on the attestation so a reader can follow
    /// it too.
    pub evidence_url: String,
    /// The person being credited.
    pub username: String,
}

/// Attest a credit on somebody else's released work.
///
/// By hand, because nothing here can see a credit roll. Restricted to audio
/// reviewers: the whole value of the attestation is that a competent person
/// followed the link.
#[utoipa::path(
    post, path = "/api/audio/deliverables/{deliverable_id}/credit", tag = "audio",
    params(("deliverable_id" = Uuid, Path, description = "The verified deliverable")),
    request_body = CreditBody,
    responses(
        (status = 200, description = "Attested", body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Needs audio review rights", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn credit(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(deliverable_id): Path<Uuid>,
    Json(body): Json<CreditBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["audio_reviewer:all", "challenge_validator:audio", "admin"],
    )
    .await?;

    let user_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&body.username)
        .fetch_optional(&state.db)
        .await?;
    let user_id = user_id.ok_or_else(|| AppError::NotFound("no such account".into()))?;

    let id =
        audio_attestations::issue_credit(&state.db, user_id, deliverable_id, &body.evidence_url)
            .await?;

    Ok(Json(ApiResponse::new(json!({
        "attestation_id": id,
        // `null` when the credit was already attested. Not an error: the
        // second reviewer to check the same credit reached the same
        // conclusion, and re-running has to be free.
        "already_attested": id.is_none(),
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Mentors
// ═══════════════════════════════════════════════════════════════════

/// Mentors worth suggesting to the caller, best first, with the reasoning.
///
/// The same module the code and AI domains use. What differs is three strings:
/// which domain to score, which answer holds the tools, and how many mentees is
/// too many. Audio caps a mentor at three, like AI and for a related reason —
/// listening to somebody's mix and saying something useful about it is not a
/// fifteen-minute pass over a diff.
#[utoipa::path(
    get, path = "/api/audio/mentors/for-me", tag = "audio",
    responses(
        (status = 200, description = "Suggested mentors", body = ApiResponse<Vec<crate::services::mentorship_matching::Match>>),
        (status = 400, description = "Audio onboarding not answered", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn mentor_matches(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Vec<crate::services::mentorship_matching::Match>>>, AppError> {
    let matches = crate::services::mentorship_matching::matches_for(
        &state.db,
        crate::services::mentorship_matching::AUDIO,
        auth.user_id,
        10,
    )
    .await?;
    Ok(Json(ApiResponse::new(matches)))
}

// ═══════════════════════════════════════════════════════════════════
// External portfolios
// ═══════════════════════════════════════════════════════════════════
//
// Declared rather than fetched, and the row says so. SoundCloud closed its API
// to new applications in 2019, Bandcamp never had one, and Voice123 and Casting
// Call Club publish nothing machine-readable. Every one of them is where this
// domain's careers actually live.
//
// Refusing the ones that cannot be checked would erase the recorded work of
// most musicians on the platform. Accepting their figures as verified would
// make the craft score a self-assessment. So they are accepted, marked, and
// counted at half weight by `audio_profile::reach` — the only one of the three
// that is honest about what it knows.

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct PortfolioRow {
    pub id: Uuid,
    pub platform: String,
    pub handle: String,
    pub profile_url: String,
    pub items_count: Option<i32>,
    pub reach_count: Option<i64>,
    pub figures_are_declared: bool,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// The audio accounts the caller has linked.
#[utoipa::path(
    get, path = "/api/audio/portfolios", tag = "audio",
    responses((status = 200, description = "Linked accounts", body = ApiResponse<Vec<PortfolioRow>>)),
    security(("cookie_auth" = [])),
)]
pub async fn my_portfolios(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Vec<PortfolioRow>>>, AppError> {
    let rows: Vec<PortfolioRow> = sqlx::query_as(
        "SELECT p.id, p.platform, p.handle, p.profile_url, p.items_count,
                p.reach_count, p.figures_are_declared, p.verified_at
           FROM user_external_portfolios p
           JOIN portfolio_platforms pf ON pf.slug = p.platform
          WHERE p.user_id = $1 AND pf.skill_domain = 'audio'
          ORDER BY pf.sort_order",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeclarePortfolioBody {
    /// One of the audio rows of `portfolio_platforms`: `soundcloud`,
    /// `bandcamp`, `freesound`, `opengameart`, `voice123`, `castingcallclub`,
    /// `bandlab`.
    pub platform: String,
    pub handle: String,
    pub profile_url: String,
    /// Tracks, sounds or roles — whatever the platform's `items_label` says.
    pub items_count: Option<i32>,
    /// Plays or downloads, where the platform shows them.
    pub reach_count: Option<i64>,
}

/// Link an audio account, with the figures the person reads on it.
#[utoipa::path(
    post, path = "/api/audio/portfolios", tag = "audio",
    request_body = DeclarePortfolioBody,
    responses(
        (status = 200, description = "Linked", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Not an audio platform", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn declare_portfolio(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DeclarePortfolioBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    crate::validators::check_max_len(&body.handle, "handle", 120)?;
    crate::validators::validate_url(&body.profile_url, "profile_url", 500)?;
    if !body.profile_url.starts_with("https://") {
        return Err(AppError::Validation(
            "profile_url must start with https://".into(),
        ));
    }
    if body.items_count.is_some_and(|n| n < 0) || body.reach_count.is_some_and(|n| n < 0) {
        return Err(AppError::Validation("a count cannot be negative".into()));
    }

    let known: Option<String> = sqlx::query_scalar(
        "SELECT slug FROM portfolio_platforms
          WHERE slug = $1 AND skill_domain = 'audio'",
    )
    .bind(&body.platform)
    .fetch_optional(&state.db)
    .await?;

    if known.is_none() {
        let options: Vec<String> = sqlx::query_scalar(
            "SELECT slug FROM portfolio_platforms WHERE skill_domain = 'audio' ORDER BY sort_order",
        )
        .fetch_all(&state.db)
        .await?;
        return Err(AppError::Validation(format!(
            "platform must be one of: {}",
            options.join(", ")
        )));
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO user_external_portfolios
            (user_id, platform, handle, profile_url, items_count, reach_count,
             figures_are_declared, sync_enabled)
        VALUES ($1, $2, $3, $4, $5, $6, TRUE, FALSE)
        ON CONFLICT (user_id, platform, handle) DO UPDATE
            SET profile_url = EXCLUDED.profile_url,
                items_count = EXCLUDED.items_count,
                reach_count = EXCLUDED.reach_count,
                updated_at = NOW()
        RETURNING id
        "#,
    )
    .bind(auth.user_id)
    .bind(&body.platform)
    .bind(body.handle.trim())
    .bind(body.profile_url.trim())
    .bind(body.items_count)
    .bind(body.reach_count)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({
        "id": id,
        // Said out loud, because it changes what the figure is worth: nothing
        // on these platforms can be checked automatically.
        "figures_are_declared": true,
    }))))
}

/// Unlink an account.
#[utoipa::path(
    delete, path = "/api/audio/portfolios/{id}", tag = "audio",
    params(("id" = Uuid, Path, description = "Portfolio row")),
    responses(
        (status = 200, description = "Removed", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Not the caller's", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn drop_portfolio(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let done = sqlx::query("DELETE FROM user_external_portfolios WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("portfolio not found".into()));
    }
    Ok(Json(ApiResponse::new(json!({ "removed": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// Credits
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct CreditRow {
    pub username: String,
    pub display_name: Option<String>,
    /// The words the attestation was issued with.
    pub credit_title: String,
    /// What was made, when the slice said.
    pub audio_subtype: Option<String>,
    /// The public attestation, so a reader can check the credit rather than
    /// take the page's word for it.
    pub verification_code: String,
    pub issued_at: chrono::DateTime<chrono::Utc>,
}

/// Who is credited on a project.
///
/// Public and unauthenticated: a credits list that needs an account is a
/// credits list nobody outside reads, and being readable from outside is the
/// entire point of a credit.
///
/// Reads `work_credits` (migration 0523), which excludes revoked attestations
/// and revoked deliverables — a credit the platform has retracted leaves the
/// page it was printed on, without anybody editing the page.
#[utoipa::path(
    get, path = "/api/projects/{slug}/credits", tag = "audio",
    params(("slug" = String, Path, description = "Project slug")),
    responses(
        (status = 200, description = "Credits", body = ApiResponse<Vec<CreditRow>>),
    ),
)]
pub async fn project_credits(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<Vec<CreditRow>>>, AppError> {
    let rows: Vec<CreditRow> = sqlx::query_as(
        "SELECT username, display_name, credit_title, audio_subtype,
                verification_code, issued_at
           FROM work_credits
          WHERE project_slug = $1
          ORDER BY issued_at",
    )
    .bind(&slug)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}
