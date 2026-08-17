//! The code profile — one request, everything a reader needs to judge
//! somebody's work without taking the platform's word for it.
//!
//! The score is first because it is what makes a list sortable, and the
//! breakdown is right behind it because a score with no explanation is a
//! number somebody has to trust. Then the artefacts themselves, which are the
//! actual answer: the score is a summary of these, and anybody who wants to
//! check it can follow the links.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::craft_score;

pub fn code_profile_routes() -> Router<AppState> {
    Router::new()
        .route("/users/{username}/code-profile", get(code_profile))
        .route("/users/me/code-profile/recompute", post(recompute_mine))
        .route("/code/tiers", get(list_tiers))
        .route(
            "/users/me/code-portfolios",
            get(my_portfolios).post(claim_portfolio),
        )
        .route(
            "/users/me/code-portfolios/{id}",
            axum::routing::delete(drop_portfolio),
        )
        .route("/code/onboarding", post(complete_onboarding))
        .route("/code/onboarding/skip", post(skip_onboarding))
        .route("/code/mentors/for-me", get(mentor_matches))
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

/// The person, and what the sorted lists currently believe about them.
#[derive(sqlx::FromRow)]
struct ProfileHeader {
    id: Uuid,
    profile_hidden: bool,
    /// Absent means never computed, which is not the same as zero.
    score: Option<i32>,
    computed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct AttestationSummary {
    pub id: Uuid,
    pub basis: Option<String>,
    pub title: String,
    pub description: String,
    /// The code a reader types into the public verification page.
    pub verification_code: String,
    pub issued_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct LanguageShare {
    pub language: String,
    pub artefacts: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct PublishedPackage {
    pub registry: String,
    pub package_name: String,
    pub latest_version: Option<String>,
    pub downloads_recent: Option<i64>,
    pub downloads_total: Option<i64>,
    /// When the figures were last read. Shown, because a number with no date
    /// is a number nobody can weigh.
    pub fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Everything on somebody's code profile, in one request.
///
/// Public: the whole point of the platform is that this can be sent to
/// somebody who has no account. A hidden profile answers 404 rather than an
/// empty one, so the absence cannot be read as "this person has done nothing".
#[utoipa::path(
    get, path = "/api/users/{username}/code-profile", tag = "profile",
    params(("username" = String, Path, description = "Username")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such profile", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn code_profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    let user: Option<ProfileHeader> = sqlx::query_as(
        "SELECT u.id, u.profile_hidden, cs.score, cs.computed_at
           FROM users u
           LEFT JOIN craft_scores cs
                  ON cs.user_id = u.id AND cs.skill_domain = $2
          WHERE u.username = $1 AND u.is_banned = FALSE",
    )
    .bind(&username)
    .bind(craft_score::DOMAIN)
    .fetch_optional(&state.db)
    .await?;
    let ProfileHeader {
        id: user_id,
        profile_hidden: hidden,
        score: stored_score,
        computed_at,
    } = user.ok_or_else(|| AppError::NotFound("profile not found".into()))?;
    if hidden {
        return Err(AppError::NotFound("profile not found".into()));
    }

    // Computed live rather than read from the stored row: the row exists so
    // lists can be sorted, and this is the one page where being an hour out
    // of date would be visible to the person it is about.
    let score = craft_score::compute(&state.db, user_id).await?;

    let attestations = sqlx::query_as::<_, AttestationSummary>(
        "SELECT id, basis, title, description, verification_code, issued_at
           FROM attestations
          WHERE user_id = $1 AND revoked_at IS NULL AND public = TRUE
            AND basis IS NOT NULL
          ORDER BY issued_at DESC
          LIMIT 100",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    let languages = sqlx::query_as::<_, LanguageShare>(
        r#"
        SELECT picked.lang AS language, count(*) AS artefacts
          FROM deliverables d
          LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
          LEFT JOIN project_slices s ON s.id = d.slice_id
          CROSS JOIN LATERAL (
              SELECT COALESCE(
                  NULLIF(ct.language, ''),
                  (SELECT sl FROM unnest(s.code_languages) AS sl LIMIT 1)
              ) AS lang
          ) AS picked
         WHERE d.user_id = $1
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
           AND picked.lang IS NOT NULL
         GROUP BY picked.lang
         ORDER BY count(*) DESC, picked.lang
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    let packages = sqlx::query_as::<_, PublishedPackage>(
        r#"
        SELECT DISTINCT ON (ps.registry, ps.package_name)
               ps.registry, ps.package_name, ps.latest_version,
               ps.downloads_recent, ps.downloads_total, ps.fetched_at
          FROM published_artifact_stats ps
          JOIN deliverables d ON d.slice_id = ps.slice_id
         WHERE d.user_id = $1
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
         ORDER BY ps.registry, ps.package_name, ps.fetched_at DESC NULLS LAST
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    let orientations: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT o.slug, o.name, uo.is_primary
           FROM user_orientations uo
           JOIN orientations o ON o.id = uo.orientation_id
          WHERE uo.user_id = $1 AND uo.ended_at IS NULL
            AND o.primary_domain = 'code'
          ORDER BY uo.is_primary DESC, o.name",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    let missions: Vec<(String, i64)> = sqlx::query_as(
        "SELECT mt.slug, count(*)
           FROM missions m
           JOIN mission_types mt ON mt.id = m.mission_type_id
          WHERE m.assigned_user_id = $1 AND m.status = 'closed'
          GROUP BY mt.slug
          ORDER BY count(*) DESC",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    // Both proved and claimed. A claimed Codeberg account is worth a link on
    // the page even though it earns nothing, and the page says which is which.
    let portfolios = crate::services::code_portfolio::for_user(&state.db, user_id).await?;
    let portfolios: Vec<Value> = portfolios
        .into_iter()
        .map(|p| {
            let verified = p.verified_at.is_some();
            json!({
                "platform": p.platform,
                "handle": p.handle,
                "profile_url": p.profile_url,
                "verified": verified,
                "repos_count": p.repos_count,
                "stars_received": p.stars_received,
                "followers_count": p.followers_count,
                "last_synced_at": p.last_synced_at,
            })
        })
        .collect();

    Ok(Json(build_response(json!({
        "username": username,
        "craft_score": score,
        "portfolios": portfolios,
        // What the sorted lists are using. Shown next to the live figure so a
        // discrepancy is visible rather than confusing.
        "stored_score": stored_score,
        "stored_score_computed_at": computed_at,
        "orientations": orientations
            .into_iter()
            .map(|(slug, name, primary)| json!({
                "slug": slug, "name": name, "is_primary": primary
            }))
            .collect::<Vec<_>>(),
        "attestations": attestations,
        "languages": languages,
        "published_packages": packages,
        "missions_completed": missions
            .into_iter()
            .map(|(kind, count)| json!({ "mission_type": kind, "count": count }))
            .collect::<Vec<_>>(),
    }))))
}

/// Recompute and store the caller's own score.
///
/// The sweep runs hourly; this exists so somebody who has just had an
/// attestation issued does not have to wait an hour to see it counted.
#[utoipa::path(
    post, path = "/api/users/me/code-profile/recompute", tag = "profile",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn recompute_mine(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let score = craft_score::recompute(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "craft_score": score }))))
}

// ─── Onboarding ──────────────────────────────────────────────────

/// Answer the seven questions and get a first month back.
///
/// The recommendation is returned rather than only stored: somebody who has
/// just answered wants to see what it changed, and a wizard that ends on a
/// confirmation screen is one people skip next time.
#[utoipa::path(
    post, path = "/api/code/onboarding", tag = "profile",
    request_body(content = serde_json::Value, description = "WizardAnswers"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unknown answer, or everything selected", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn complete_onboarding(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(answers): Json<crate::services::code_onboarding::WizardAnswers>,
) -> Result<Json<Value>, AppError> {
    let recommendation =
        crate::services::code_onboarding::complete(&state.db, auth.user_id, &answers).await?;

    // A GitHub username given here means "import what I already have", and
    // waiting for the next weekly sweep would make the wizard look inert.
    // Claimed, not verified — typing a name proves nothing, and connecting
    // the account is a separate, deliberate act.
    if let Some(username) = answers.github_username.as_deref().map(str::trim)
        && !username.is_empty()
    {
        let url = format!("https://github.com/{username}");
        if let Err(err) =
            crate::services::code_portfolio::claim(&state.db, auth.user_id, &url).await
        {
            tracing::info!(%err, "github username from onboarding not recorded");
        }
    }

    Ok(Json(build_response(
        json!({ "recommendation": recommendation }),
    )))
}

/// Stop asking.
#[utoipa::path(
    post, path = "/api/code/onboarding/skip", tag = "profile",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn skip_onboarding(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::services::code_onboarding::skip(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "skipped": true }))))
}

/// Mentors worth suggesting, with the reasoning attached.
#[utoipa::path(
    get, path = "/api/code/mentors/for-me", tag = "profile",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Onboarding not answered", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn mentor_matches(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let matches =
        crate::services::code_mentorship::matches_for(&state.db, auth.user_id, 10).await?;
    Ok(Json(build_response(json!({ "mentors": matches }))))
}

// ─── Portfolios on other platforms ───────────────────────────────

#[derive(Debug, serde::Deserialize, ToSchema)]
pub struct ClaimBody {
    /// The profile page itself — `https://codeberg.org/someone`, not one of
    /// its repositories.
    #[schema(max_length = 500)]
    pub profile_url: String,
}

/// Accounts on other platforms, proved and claimed.
#[utoipa::path(
    get, path = "/api/users/me/code-portfolios", tag = "profile",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_portfolios(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let portfolios = crate::services::code_portfolio::for_user(&state.db, auth.user_id).await?;
    let rows: Vec<Value> = portfolios
        .into_iter()
        .map(|p| {
            let countable =
                crate::services::code_portfolio::is_countable(&p.platform, p.verified_at.is_some());
            let mut value = serde_json::to_value(&p).unwrap_or_else(|_| json!({}));
            if let Some(object) = value.as_object_mut() {
                // Said explicitly rather than left to be inferred from
                // `verified_at`: a claimed account still shows on the page,
                // and somebody should be able to see why it earns nothing.
                object.insert("counts_towards_score".into(), json!(countable));
            }
            value
        })
        .collect();
    Ok(Json(build_response(json!({ "portfolios": rows }))))
}

/// Claim an account on a platform Skilluv cannot prove.
///
/// The row is stored unverified and shows on the profile as a link. GitHub
/// goes through OAuth instead, which is the only path to a countable row.
#[utoipa::path(
    post, path = "/api/users/me/code-portfolios", tag = "profile",
    request_body = ClaimBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a forge profile, or already proved by somebody else", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn claim_portfolio(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ClaimBody>,
) -> Result<Json<Value>, AppError> {
    crate::validators::check_max_len(&body.profile_url, "profile_url", 500)?;
    let portfolio =
        crate::services::code_portfolio::claim(&state.db, auth.user_id, &body.profile_url).await?;

    // Fetched now rather than at the next weekly sweep: somebody who has
    // just added an account expects to see something.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("no HTTP client: {e}")))?;
    let _ = crate::services::code_portfolio::sync_one(&state.db, &client, portfolio.id).await;

    let refreshed = crate::services::code_portfolio::for_user(&state.db, auth.user_id)
        .await?
        .into_iter()
        .find(|p| p.id == portfolio.id);

    Ok(Json(build_response(json!({ "portfolio": refreshed }))))
}

/// Remove an account from the profile.
#[utoipa::path(
    delete, path = "/api/users/me/code-portfolios/{id}", tag = "profile",
    params(("id" = Uuid, Path, description = "Portfolio id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not the caller's", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn drop_portfolio(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let done = sqlx::query("DELETE FROM user_code_portfolios WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("portfolio not found".into()));
    }
    Ok(Json(build_response(json!({ "removed": true }))))
}

/// The tiers and where each one starts. Public, because a threshold nobody
/// can read is a threshold nobody can aim at.
#[utoipa::path(
    get, path = "/api/code/tiers", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_tiers(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let tiers: Vec<(String, String, i32, Option<i32>, String)> = sqlx::query_as(
        "SELECT slug, name, min_score, max_score, description
           FROM craft_score_tiers WHERE skill_domain = 'code'
          ORDER BY sort_order",
    )
    .fetch_all(&state.db)
    .await?;

    let weights: Vec<(String, bigdecimal::BigDecimal, String, String)> = sqlx::query_as(
        "SELECT term, weight, kind, explanation
           FROM craft_score_weights
          WHERE skill_domain = 'code' AND is_active = TRUE
          ORDER BY sort_order",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "cap": craft_score::CAP,
        "tiers": tiers
            .into_iter()
            .map(|(slug, name, min, max, description)| json!({
                "slug": slug, "name": name,
                "min_score": min, "max_score": max,
                "description": description,
            }))
            .collect::<Vec<_>>(),
        // The formula, published. A score computed from a secret formula is
        // a score people game by guessing rather than by doing the work.
        "formula": weights
            .into_iter()
            .map(|(term, weight, kind, explanation)| json!({
                "term": term,
                "weight": weight.to_string(),
                "kind": kind,
                "explanation": explanation,
            }))
            .collect::<Vec<_>>(),
    }))))
}
