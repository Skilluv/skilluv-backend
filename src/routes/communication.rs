//! Communication work: the profile, the translation reviews, and what a
//! published piece reports back.
//!
//! ## What is not here
//!
//! The toolkit, the onboarding guides, the brief templates and the writeup
//! templates. They are `content_guides` rows and
//! `/api/guides?domain=communication` already serves them — a second endpoint
//! would be the mistake `routes::guides` was written to undo.
//!
//! The challenge catalogue, the contests, the missions and the badges are the
//! platform's own, keyed by domain. Communication adds rows to them and no
//! routes.
//!
//! The revision rounds are `routes::slice_revisions` and the external accounts
//! are `routes::portfolios`, both of which serve every domain. Audio had
//! copies of each; they were removed rather than duplicated a third time.
//!
//! ## What is genuinely specific to this domain
//!
//! Translation review. Nothing in this database can tell a good translation
//! from a fluent wrong one, and the only instrument is a person who reads both
//! languages. So a translation is the one artefact here that is never attested
//! automatically: somebody declares the languages they read, and signs a
//! review in one of them.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::communication_attestations;
use crate::services::communication_profile;

pub fn communication_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/users/{username}/communication-profile",
            get(user_communication_profile),
        )
        .route(
            "/communication/review-languages",
            get(my_review_languages).post(declare_review_language),
        )
        .route(
            "/communication/review-languages/{language}",
            axum::routing::delete(drop_review_language),
        )
        .route(
            "/communication/slices/{slice_id}/translation-reviews",
            get(list_translation_reviews).post(review_translation),
        )
        .route(
            "/communication/slices/{slice_id}/publications",
            get(list_publications),
        )
        .route("/communication/mentors/for-me", get(mentor_matches))
}

// ═══════════════════════════════════════════════════════════════════
// GET /users/{username}/communication-profile
// ═══════════════════════════════════════════════════════════════════

/// Everything one person has to show in the communication trades.
#[utoipa::path(
    get, path = "/api/users/{username}/communication-profile", tag = "communication",
    params(("username" = String, Path, description = "Public username")),
    responses(
        (status = 200, description = "Profile", body = ApiResponse<communication_profile::CommunicationProfile>),
        (status = 404, description = "No such user", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn user_communication_profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<communication_profile::CommunicationProfile>>, AppError> {
    let profile = communication_profile::build(&state.db, &username).await?;
    Ok(Json(ApiResponse::new(profile)))
}

// ═══════════════════════════════════════════════════════════════════
// Review languages
// ═══════════════════════════════════════════════════════════════════
//
// Declared, never proven, and migration 0516 says why: nothing here can test
// somebody's Swahili, and a quiz would produce a number that looks like
// evidence. What the declaration buys is accountability — it is signed, and
// every review made under it carries it.

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct ReviewLanguageRow {
    /// A BCP 47 tag: `fr`, `pt-BR`, `sw`, `ar`, `wo`.
    pub language: String,
    pub proficiency: String,
    pub note: String,
    pub declared_at: chrono::DateTime<chrono::Utc>,
}

/// The languages the caller has said they can review in.
#[utoipa::path(
    get, path = "/api/communication/review-languages", tag = "communication",
    responses((status = 200, description = "Declared languages", body = ApiResponse<Vec<ReviewLanguageRow>>)),
    security(("cookie_auth" = [])),
)]
pub async fn my_review_languages(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Vec<ReviewLanguageRow>>>, AppError> {
    let rows: Vec<ReviewLanguageRow> = sqlx::query_as(
        "SELECT language, proficiency, note, declared_at
           FROM user_review_languages WHERE user_id = $1
          ORDER BY language",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeclareLanguageBody {
    /// A BCP 47 tag. Not checked against a list of languages: a closed
    /// vocabulary here would be a statement about which languages exist.
    pub language: String,
    /// `native`, `bilingual` or `professional`.
    pub proficiency: Option<String>,
    /// Anything worth saying about the claim, in the person's own words.
    pub note: Option<String>,
}

/// Declare that you read a language well enough to review a translation in it.
///
/// This is a signed statement, not a permission: the right to review
/// translations is `communication_reviewer:translation`, granted the normal
/// way. This says which languages that right can honestly be used in.
#[utoipa::path(
    post, path = "/api/communication/review-languages", tag = "communication",
    request_body = DeclareLanguageBody,
    responses(
        (status = 200, description = "Declared", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Not a language tag", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn declare_review_language(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DeclareLanguageBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let language = body.language.trim();
    crate::validators::check_max_len(language, "language", 20)?;

    let proficiency = body.proficiency.as_deref().unwrap_or("professional");
    if !matches!(proficiency, "native" | "bilingual" | "professional") {
        return Err(AppError::Validation(
            "proficiency must be one of: native, bilingual, professional".into(),
        ));
    }
    let note = body.note.as_deref().unwrap_or("");
    crate::validators::check_max_len(note, "note", 500)?;

    // The tag shape is checked by the column's own CHECK. Turning its failure
    // into a message rather than a 500: an operator error and a bad request
    // look identical to a constraint violation otherwise.
    if !is_language_tag(language) {
        return Err(AppError::Validation(
            "language must be a BCP 47 tag, such as fr, pt-BR, sw or wo".into(),
        ));
    }

    sqlx::query(
        "INSERT INTO user_review_languages (user_id, language, proficiency, note)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, language) DO UPDATE
             SET proficiency = EXCLUDED.proficiency,
                 note = EXCLUDED.note,
                 declared_at = NOW()",
    )
    .bind(auth.user_id)
    .bind(language)
    .bind(proficiency)
    .bind(note.trim())
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({ "language": language }))))
}

/// Withdraw a language declaration.
///
/// Reviews already signed under it stay: they were true when they were made,
/// and deleting them would remove the trace of a claim somebody relied on.
#[utoipa::path(
    delete, path = "/api/communication/review-languages/{language}", tag = "communication",
    params(("language" = String, Path, description = "BCP 47 tag")),
    responses(
        (status = 200, description = "Withdrawn", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Not declared", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn drop_review_language(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(language): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let done =
        sqlx::query("DELETE FROM user_review_languages WHERE user_id = $1 AND language = $2")
            .bind(auth.user_id)
            .bind(language.trim())
            .execute(&state.db)
            .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("language not declared".into()));
    }
    Ok(Json(ApiResponse::new(json!({ "withdrawn": true }))))
}

/// Whether a string looks like a BCP 47 tag.
///
/// Deliberately shallow: two or three letters, then optional subtags. It
/// rejects a sentence and accepts a language nobody has heard of, which is the
/// right way round — the alternative is a list of the languages the platform
/// has decided exist.
fn is_language_tag(s: &str) -> bool {
    let mut parts = s.split('-');
    let Some(primary) = parts.next() else {
        return false;
    };
    if !(2..=3).contains(&primary.len()) || !primary.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    parts.all(|p| (2..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

// ═══════════════════════════════════════════════════════════════════
// Translation reviews
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct TranslationReviewRow {
    pub id: Uuid,
    pub reviewer_username: String,
    pub language: String,
    pub proficiency: Option<String>,
    pub notes_md: String,
    pub reviewed_at: chrono::DateTime<chrono::Utc>,
}

/// Who has read this translation, in which language, and what they said.
///
/// Public: the point of the record is that a reader weighing the attestation
/// can see whose word it rests on.
#[utoipa::path(
    get, path = "/api/communication/slices/{slice_id}/translation-reviews", tag = "communication",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    responses((status = 200, description = "Reviews", body = ApiResponse<Vec<TranslationReviewRow>>)),
)]
pub async fn list_translation_reviews(
    State(state): State<AppState>,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<TranslationReviewRow>>>, AppError> {
    let rows: Vec<TranslationReviewRow> = sqlx::query_as(
        r#"
        SELECT tr.id, u.username AS reviewer_username, tr.language,
               rl.proficiency, tr.notes_md, tr.reviewed_at
          FROM translation_reviews tr
          JOIN users u ON u.id = tr.reviewer_user_id
          LEFT JOIN user_review_languages rl
                 ON rl.user_id = tr.reviewer_user_id AND rl.language = tr.language
         WHERE tr.slice_id = $1
         ORDER BY tr.reviewed_at
        "#,
    )
    .bind(slice_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewTranslationBody {
    /// One of the slice's target languages, and one the caller has declared.
    pub language: String,
    pub notes_md: Option<String>,
}

/// Validate a translation, in a language you have declared you read.
///
/// The capability is checked here; everything else — that the slice is a
/// translation, that it targets this language, that the caller is not the
/// translator, that they declared the language — is checked in
/// [`communication_attestations::validate_translation`], which is the only
/// door to the basis.
#[utoipa::path(
    post, path = "/api/communication/slices/{slice_id}/translation-reviews", tag = "communication",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    request_body = ReviewTranslationBody,
    responses(
        (status = 200, description = "Reviewed", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Not reviewable as asked", body = crate::api_response::ErrorResponse),
        (status = 403, description = "No translation review rights", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn review_translation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
    Json(body): Json<ReviewTranslationBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &[
            "communication_reviewer:translation",
            "communication_reviewer:all",
            "admin",
        ],
    )
    .await?;

    let issued = communication_attestations::validate_translation(
        &state.db,
        auth.user_id,
        slice_id,
        &body.language,
        body.notes_md.as_deref().unwrap_or(""),
    )
    .await?;

    Ok(Json(ApiResponse::new(json!({
        // Absent when the artefact already carried the attestation: reviewing
        // twice is allowed and issues nothing twice.
        "attestation_id": issued,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// What a published piece reports back
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct PublicationRow {
    pub registry: String,
    pub registry_name: String,
    /// The identifier on that platform: an article slug, a video id, a DOI.
    pub package_name: String,
    pub views_count: Option<i64>,
    pub engagement_count: Option<i32>,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    /// What went wrong on the last attempt, when something did. A figure with
    /// a stale date and a visible error is worth more than a figure with
    /// neither.
    pub last_error: Option<String>,
}

/// The figures the platforms have reported for this piece.
///
/// Ticket W-03 asked for a `communication_external_pubs` table. This reads
/// `published_artifact_stats`, which migration 0181 built for exactly this
/// question and 0507 widened with `views_count` and `engagement_count`. A
/// second table would have meant a second sweep, a second staleness rule and
/// two answers to how far a piece travelled.
#[utoipa::path(
    get, path = "/api/communication/slices/{slice_id}/publications", tag = "communication",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    responses((status = 200, description = "Reported figures", body = ApiResponse<Vec<PublicationRow>>)),
)]
pub async fn list_publications(
    State(state): State<AppState>,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<PublicationRow>>>, AppError> {
    let rows: Vec<PublicationRow> = sqlx::query_as(
        "SELECT s.registry, r.name AS registry_name, s.package_name,
                s.views_count, s.engagement_count, s.published_at,
                s.fetched_at, s.last_error
           FROM published_artifact_stats s
           JOIN publication_registries r ON r.slug = s.registry
          WHERE s.slice_id = $1
          ORDER BY r.sort_order",
    )
    .bind(slice_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(rows)))
}

// ═══════════════════════════════════════════════════════════════════
// Mentors
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct MentorQuery {
    /// How many to return. Ten by default.
    pub limit: Option<i64>,
}

/// Mentors worth suggesting to the caller, best first, with the reasoning.
///
/// The same module the code, AI, ops, audio and design domains use. What
/// differs is three strings: which domain to score, which answer holds the
/// tools, and how many mentees is too many.
#[utoipa::path(
    get, path = "/api/communication/mentors/for-me", tag = "communication",
    params(MentorQuery),
    responses((status = 200, description = "Suggestions", body = ApiResponse<Vec<crate::services::mentorship_matching::Match>>)),
    security(("cookie_auth" = [])),
    // Every domain has a handler of this name, so the generated id has to
    // carry the domain: two operations sharing one is a client generator
    // silently dropping one of them.
    operation_id = "communicationMentorMatches",
)]
pub async fn mentor_matches(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<MentorQuery>,
) -> Result<Json<ApiResponse<Vec<crate::services::mentorship_matching::Match>>>, AppError> {
    let limit = q.limit.unwrap_or(10).clamp(1, 50);
    let matches = crate::services::mentorship_matching::matches_for(
        &state.db,
        crate::services::mentorship_matching::COMMUNICATION,
        auth.user_id,
        limit,
    )
    .await?;
    Ok(Json(ApiResponse::new(matches)))
}

#[cfg(test)]
mod tests {
    use super::is_language_tag;

    #[test]
    fn ordinary_tags_pass() {
        for tag in ["fr", "en", "sw", "wo", "ln", "pt-BR", "zh-Hant-TW", "ber"] {
            assert!(is_language_tag(tag), "{tag} should be accepted");
        }
    }

    #[test]
    fn a_language_nobody_has_heard_of_passes() {
        // Deliberate. A closed list here would be a statement about which
        // languages the platform believes exist, and this platform is built
        // for a continent with two thousand of them.
        assert!(is_language_tag("dyu"));
        assert!(is_language_tag("bci"));
    }

    #[test]
    fn a_sentence_does_not() {
        for junk in ["", "f", "french please", "toolongprimary", "fr-", "-fr"] {
            assert!(!is_language_tag(junk), "{junk:?} should be refused");
        }
    }
}
