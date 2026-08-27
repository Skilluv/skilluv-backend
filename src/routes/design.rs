//! The design critique loop, over HTTP.
//!
//! A designer hands in a version, a reviewer answers with one of three
//! verdicts, anybody can read the whole trail or set two rounds against each
//! other, and a reviewer sees the queue of what they are competent to judge.
//!
//! Contests are not here. A design contest is a `brief_contest` on the
//! tournament routes, because a contest is the same event whatever the
//! subject — see `services::contest`.
//!
//! Authorisation lives in the service (`design_reviewer:{group}` resolved
//! from the slice's trade) and in the database (the five-round ceiling, the
//! blocking-reason rules). This layer validates payload shape and nothing
//! else, so there is one place per rule.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::design_auto_checks;
use crate::services::design_reviews::{self, ReviewInput, Verdict};
use crate::services::next_challenges as next_challenges_service;

/// Long enough for a reviewer's morning, short enough that nobody paginates
/// through a queue whose whole point is "the oldest wait first".
const MAX_QUEUE_LIMIT: i64 = 100;

pub fn design_routes() -> Router<AppState> {
    Router::new()
        .route("/design/slices/{id}/versions", post(submit_version))
        .route("/design/slices/{id}/reviews", get(history).post(review))
        .route("/design/slices/{id}/compare", get(compare))
        .route("/design/slices/{id}/versions/{round}", get(version_at))
        .route("/design/slices/{id}/auto-checks", get(auto_checks))
        .route("/design/reviews/queue", get(reviewer_queue))
        .route("/users/me/next-challenges", get(next_challenges))
        .route(
            "/design/users/{username}/iteration-stories",
            get(iteration_stories),
        )
}

fn wrap(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// ═══════════════════════════════════════════════════════════════════
// POST /design/slices/{id}/versions
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitVersionBody {
    /// Where this version lives: a versioned Figma node, a hosted board, a
    /// published project, or a stored object.
    pub artifact_url: String,
    /// What changed since the previous version. Optional on the first one,
    /// and the single most useful thing to write on any later one.
    #[serde(default)]
    pub notes_md: Option<String>,
}

/// Hand in a version and ask for a critique.
#[utoipa::path(
    post, path = "/api/design/slices/{id}/versions", tag = "design",
    params(("id" = Uuid, Path, description = "design slice id")),
    request_body = SubmitVersionBody,
    responses(
        (status = 201, description = "version recorded, the slice now waits for a critique"),
        (status = 403, description = "not the designer who claimed this challenge"),
        (status = 409, description = "the slice is not in a state a version can be handed in from"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit_version(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitVersionBody>,
) -> Result<impl IntoResponse, AppError> {
    let slice = design_reviews::submit_version(
        &state.db,
        id,
        auth.user_id,
        &body.artifact_url,
        body.notes_md.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(wrap(json!({ "slice": slice })))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /design/slices/{id}/reviews
// ═══════════════════════════════════════════════════════════════════

/// The whole critique trail, oldest round first.
///
/// Public, and deliberately so: the sequence of rounds is the most convincing
/// thing a designer can show, far more than the final image on its own. Hiding
/// it would leave the profile saying "validated" and nothing about how.
#[utoipa::path(
    get, path = "/api/design/slices/{id}/reviews", tag = "design",
    params(("id" = Uuid, Path, description = "design slice id")),
    responses((status = 200, description = "every round: what was read, what was found")),
)]
pub async fn history(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let rounds = design_reviews::history(&state.db, id).await?;
    Ok(Json(wrap(json!({ "rounds": rounds }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /design/slices/{id}/reviews
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = DesignReviewBody)]
pub struct ReviewBody {
    /// `approve`, `iterate` or `reject`.
    pub verdict: String,
    /// What kind of problem. Required for `iterate` and `reject`.
    #[serde(default)]
    pub blocking_reason: Option<String>,
    /// The written critique. At least 40 characters when the verdict is not
    /// an approval.
    #[serde(default)]
    pub feedback_md: Option<String>,
    /// The family review grid, filled in. Free-form JSON so a grid can be
    /// revised without a migration; the criteria are in `review_grids`.
    #[serde(default)]
    pub grid_scores: Option<serde_json::Value>,
}

/// Answer a version.
///
/// Requires review rights for the slice's trade, and refuses a reviewer who
/// claimed the challenge themselves.
#[utoipa::path(
    post, path = "/api/design/slices/{id}/reviews", tag = "design",
    params(("id" = Uuid, Path, description = "design slice id")),
    request_body = ReviewBody,
    responses(
        (status = 200, description = "critique recorded and the slice moved"),
        (status = 400, description = "unknown verdict, missing reason, or feedback too short"),
        (status = 403, description = "no review rights for this trade, or own challenge"),
        (status = 409, description = "no version is waiting, or somebody answered first"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn review(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<impl IntoResponse, AppError> {
    let verdict = Verdict::parse(&body.verdict).ok_or_else(|| {
        AppError::Validation(format!(
            "unknown verdict '{}': expected approve, iterate or reject",
            body.verdict
        ))
    })?;

    let slice = design_reviews::review(
        &state.db,
        id,
        auth.user_id,
        ReviewInput {
            verdict,
            blocking_reason: body.blocking_reason.as_deref(),
            feedback_md: body.feedback_md.as_deref(),
            grid_scores: body.grid_scores,
        },
    )
    .await?;
    Ok(Json(wrap(json!({ "slice": slice }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /design/reviews/queue
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct QueueQuery {
    /// How many to return, 1..100. Defaults to 25.
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<i64>,
}

/// Versions waiting for a critique, in the trades this reviewer is competent
/// in, oldest first.
///
/// Returns an empty list rather than a refusal for somebody holding no review
/// capability: "nothing for you to do" is the honest answer, and a 403 on a
/// queue reads as a bug.
#[utoipa::path(
    get, path = "/api/design/reviews/queue", tag = "design",
    params(QueueQuery),
    responses((status = 200, description = "slices awaiting a critique, oldest first")),
    security(("cookie_auth" = [])),
)]
pub async fn reviewer_queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<QueueQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(25).clamp(1, MAX_QUEUE_LIMIT);
    let slices = design_reviews::reviewer_queue(&state.db, auth.user_id, limit).await?;
    Ok(Json(wrap(json!({ "slices": slices }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /design/slices/{id}/compare
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct CompareQuery {
    /// The earlier round.
    pub from: i16,
    /// The later one.
    pub to: i16,
}

/// Two rounds, and the critiques that ran between them.
///
/// Public for the same reason the trail is: the distance between the first
/// version and the last is the most convincing thing a designer has, and it
/// only reads as evidence if a stranger can check it.
///
/// The diff is not computed here. This has neither the pixels nor the fonts,
/// and rendering somebody's Figma node server-side would mean holding their
/// design account. What it returns is both versions, everything that was said
/// between them, and which comparison the subtype makes meaningful — so the
/// client does not have to keep its own copy of the twelve subtypes and guess
/// wrong on the interesting ones.
#[utoipa::path(
    get, path = "/api/design/slices/{id}/compare", tag = "design",
    params(("id" = Uuid, Path, description = "design slice id"), CompareQuery),
    responses(
        (status = 200, description = "both versions, the critiques between, and the diff strategy"),
        (status = 400, description = "from is not before to"),
        (status = 404, description = "no such round on this slice"),
    ),
)]
pub async fn compare(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<CompareQuery>,
) -> Result<impl IntoResponse, AppError> {
    let comparison = design_reviews::compare(&state.db, id, q.from, q.to).await?;
    Ok(Json(wrap(json!({ "comparison": comparison }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /design/slices/{id}/versions/{round}
// ═══════════════════════════════════════════════════════════════════

/// One version, as it was when it was reviewed.
#[utoipa::path(
    get, path = "/api/design/slices/{id}/versions/{round}", tag = "design",
    params(
        ("id" = Uuid, Path, description = "design slice id"),
        ("round" = i16, Path, description = "which round, starting at 1"),
    ),
    responses(
        (status = 200, description = "the version and the critique that closed it"),
        (status = 404, description = "no such round on this slice"),
    ),
)]
pub async fn version_at(
    State(state): State<AppState>,
    Path((id, round)): Path<(Uuid, i16)>,
) -> Result<impl IntoResponse, AppError> {
    let version = design_reviews::version_at(&state.db, id, round).await?;
    Ok(Json(wrap(json!({ "version": version }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /design/users/{username}/iteration-stories
// ═══════════════════════════════════════════════════════════════════

/// Validated work that took three rounds or more, newest first.
///
/// Three rather than two: two rounds is one critique and a fix, which happens
/// to everybody. Three is where a direction was questioned and the person came
/// back — which is the thing worth putting on a profile, and the thing a
/// portfolio of finished images can never show.
#[utoipa::path(
    get, path = "/api/design/users/{username}/iteration-stories", tag = "design",
    params(
        ("username" = String, Path, description = "whose profile"),
        QueueQuery,
    ),
    responses(
        (status = 200, description = "the work that was argued about and still got there"),
        (status = 404, description = "no such account"),
    ),
)]
pub async fn iteration_stories(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Query(q): Query<QueueQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("user '{username}' not found")))?;

    let limit = q.limit.unwrap_or(10).clamp(1, MAX_QUEUE_LIMIT);
    let stories = design_reviews::iteration_stories(&state.db, user_id, limit).await?;
    Ok(Json(wrap(json!({ "stories": stories }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /design/slices/{id}/auto-checks
// ═══════════════════════════════════════════════════════════════════

/// What the automatic checks found, round by round.
///
/// Public, like the critique trail: these are part of how a validation was
/// reached, and a reader who can see the verdict should be able to see what
/// the machine said about it.
///
/// Nothing here is a verdict. A version can carry an `error` and be approved,
/// and a clean run followed by a rejection is the common case — no check knows
/// whether a mark is right for a cooperative.
#[utoipa::path(
    get, path = "/api/design/slices/{id}/auto-checks", tag = "design",
    params(("id" = Uuid, Path, description = "design slice id")),
    responses((status = 200, description = "check results, oldest round first")),
)]
pub async fn auto_checks(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let checks = design_auto_checks::results_for(&state.db, id).await?;
    Ok(Json(wrap(json!({ "checks": checks }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /users/me/next-challenges
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct NextChallengesQuery {
    /// Which domain to suggest work in. Defaults to the caller's declared
    /// domain, and is refused if it names none of the seven.
    #[param(max_length = 30)]
    pub domain: Option<String>,
    #[param(minimum = 1, maximum = 20)]
    pub limit: Option<usize>,
}

/// What to spend this week on: open challenges and contests, ranked.
///
/// Challenges and contests come back in one list because they answer the same
/// question for the reader. Two lists would make a client merge two rankings
/// whose scores were never comparable, and would make "you have done three
/// contests in a row" impossible to notice.
///
/// Each suggestion carries the clauses that earned it its points. A
/// recommendation nobody can argue with is a recommendation nobody trusts. It
/// also carries `target_kind` (`"slice"` / `"tournament"`), so a client links
/// to the target by its nature rather than guessing from the format.
///
/// Called without `domain` by an account that has not finished onboarding —
/// one whose `users.skill_domain` is still null — this answers **400**, not an
/// empty list: there is no domain to suggest in. A caller that cannot depend on
/// onboarding being done should pass `domain` explicitly or handle the 400 as
/// "pick a domain first", never as an outage.
#[utoipa::path(
    get, path = "/api/users/me/next-challenges", tag = "profile",
    params(NextChallengesQuery),
    responses(
        (status = 200, description = "up to five suggestions, best first"),
        (status = 400, description = "unknown domain", body = crate::api_response::ErrorResponse),
        (status = 401, description = "unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn next_challenges(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<NextChallengesQuery>,
) -> Result<impl IntoResponse, AppError> {
    let domain = match q.domain {
        Some(domain) => {
            crate::validators::validate_skill_domain(&domain, "domain")?;
            domain
        }
        None => {
            sqlx::query_scalar::<_, Option<String>>("SELECT skill_domain FROM users WHERE id = $1")
                .bind(auth.user_id)
                .fetch_optional(&state.db)
                .await?
                .flatten()
                .ok_or_else(|| {
                    AppError::Validation(
                        "name a domain: this account has not finished onboarding".into(),
                    )
                })?
        }
    };

    let limit = q.limit.unwrap_or(next_challenges_service::SUGGESTION_COUNT);
    let key = next_challenges_service::cache_key(auth.user_id, &domain);
    let mut redis = state.redis.clone();

    // Cached for an hour. The inputs move over days, and a list that changed
    // on every page load would stop reading as advice.
    if let Ok(Some(cached)) = crate::services::cache::get_json::<
        Vec<next_challenges_service::Suggestion>,
    >(&mut redis, &key)
    .await
    {
        return Ok(Json(wrap(json!({ "suggestions": cached, "cached": true }))));
    }

    let suggestions =
        next_challenges_service::suggest(&state.db, auth.user_id, &domain, limit).await?;
    let _ = crate::services::cache::set_json(
        &mut redis,
        &key,
        &suggestions,
        next_challenges_service::CACHE_TTL_SECONDS,
    )
    .await;

    Ok(Json(wrap(
        json!({ "suggestions": suggestions, "cached": false }),
    )))
}
