//! The design profile — one request, everything a reader needs to judge
//! somebody's work without taking the platform's word for it.
//!
//! Same order as the code profile, for the same reasons: the score first
//! because it makes a list sortable, the breakdown right behind it because a
//! score with no explanation is a number somebody has to trust, then the
//! artefacts, which are the actual answer.
//!
//! One thing is here that has no code equivalent: the iteration trail. A
//! design deliverable validated at the fifth round is a better story than one
//! validated at the first, and the profile says so — it is the single piece
//! of evidence that separates somebody who can take a critique from somebody
//! who has only ever been agreed with.

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
use crate::services::design_craft_score;

/// Enough to judge somebody, few enough that the page loads. A reader who
/// wants the rest follows the portfolio.
const MAX_ARTEFACTS: i64 = 20;

pub fn design_profile_routes() -> Router<AppState> {
    Router::new()
        .route("/users/{username}/design-profile", get(design_profile))
        .route("/users/me/design-profile/recompute", post(recompute_mine))
        .route("/design/tiers", get(list_tiers))
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

#[derive(sqlx::FromRow)]
struct ProfileHeader {
    id: Uuid,
    profile_hidden: bool,
}

/// One validated design deliverable, as a reader meets it.
#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct DesignArtefact {
    pub deliverable_id: Uuid,
    pub title: String,
    pub artifact_url: String,
    /// The trade it belongs to, in the reader's terms.
    pub trade: Option<String>,
    /// What shape of thing it is: a brand kit, a motion piece, a typeface.
    pub subtype: Option<String>,
    /// How many critique rounds it took. The number this profile exists to
    /// show: converging at four is worth more than passing at one.
    pub rounds: Option<i16>,
    /// The average of the grids it received, when it received any.
    #[schema(value_type = Option<f64>)]
    pub grid_average: Option<bigdecimal::BigDecimal>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// GET /users/{username}/design-profile
#[utoipa::path(
    get, path = "/api/users/{username}/design-profile", tag = "design",
    params(("username" = String, Path, description = "the person")),
    responses(
        (status = 200, description = "score, breakdown, artefacts, contests, attestations"),
        (status = 404, description = "unknown or hidden profile"),
    ),
)]
pub async fn design_profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    // `profile_active` as well as `profile_hidden`: registration leaves an
    // account inactive until somebody fills it in, and serving one reads as
    // "this person has done nothing" rather than "this person has not
    // started". The recruiter search has always filtered on it; this page
    // did not.
    let header: Option<ProfileHeader> = sqlx::query_as(
        "SELECT id, profile_hidden FROM users
          WHERE username = $1 AND is_banned = FALSE AND profile_active = TRUE",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?;

    let header = header.ok_or_else(|| AppError::NotFound("profile not found".into()))?;
    if header.profile_hidden {
        // Hidden reads as absent rather than as forbidden: saying "this
        // person exists but you cannot see them" leaks the thing being hidden.
        return Err(AppError::NotFound("profile not found".into()));
    }

    // Computed rather than read: a profile page is looked at rarely enough
    // that a fresh number is cheaper than explaining a stale one.
    let score = design_craft_score::compute(&state.db, header.id).await?;

    let artefacts: Vec<DesignArtefact> = sqlx::query_as(
        r#"
        SELECT d.id AS deliverable_id,
               s.title,
               d.artifact_url,
               o.name AS trade,
               s.design_subtype AS subtype,
               (SELECT max(v.round) FROM slice_validation_decisions v
                 WHERE v.slice_id = d.slice_id) AS rounds,
               (SELECT avg((v.grid_scores ->> 'average')::NUMERIC)
                  FROM slice_validation_decisions v
                 WHERE v.slice_id = d.slice_id AND v.grid_scores ? 'average')
                 AS grid_average,
               d.verified_at
          FROM countable_deliverables d
          JOIN project_slices s ON s.id = d.slice_id
          LEFT JOIN orientations o ON o.id = s.orientation_id
         WHERE d.user_id = $1
           AND d.artifact_type = 'design_artifact'
           AND d.public = TRUE
         ORDER BY d.verified_at DESC NULLS LAST
         LIMIT $2
        "#,
    )
    .bind(header.id)
    .bind(MAX_ARTEFACTS)
    .fetch_all(&state.db)
    .await?;

    // Contests, with the standing rather than a participation badge.
    let contests: Vec<(String, Option<i32>, i64)> = sqlx::query_as(
        r#"
        SELECT t.name,
               p.rank,
               (SELECT count(*) FROM tournament_participants p2
                 WHERE p2.tournament_id = t.id)::bigint
          FROM tournament_participants p
          JOIN tournaments t ON t.id = p.tournament_id
         WHERE p.participant_type = 'user' AND p.participant_id = $1
           AND t.skill_domain = 'design'
           AND t.status = 'concluded'
         ORDER BY p.rank ASC NULLS LAST
         LIMIT $2
        "#,
    )
    .bind(header.id)
    .bind(MAX_ARTEFACTS)
    .fetch_all(&state.db)
    .await?;

    // The trades this person has actually been validated in. Declared
    // orientations are elsewhere and are not a claim about anything.
    let trades: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT o.name, count(*)::bigint
          FROM countable_deliverables d
          JOIN project_slices s ON s.id = d.slice_id
          JOIN orientations o ON o.id = s.orientation_id
         WHERE d.user_id = $1 AND d.artifact_type = 'design_artifact'
         GROUP BY o.name
         ORDER BY count(*) DESC
        "#,
    )
    .bind(header.id)
    .fetch_all(&state.db)
    .await?;

    let attestations: Vec<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT basis, title, verification_code
          FROM attestations
         WHERE user_id = $1
           AND revoked_at IS NULL
           AND public = TRUE
           -- Parenthesised on purpose: `A AND B AND C OR D` binds as
           -- `(A AND B AND C) OR D`, and the editorial branch would then
           -- escape both the revocation and the visibility filter.
           AND (basis LIKE 'design\_%' OR basis = 'featured_designer')
         ORDER BY issued_at DESC
         LIMIT $2
        "#,
    )
    .bind(header.id)
    .bind(MAX_ARTEFACTS)
    .fetch_all(&state.db)
    .await?;

    // Paid work: how much, of what kind, and how it was rated. No client
    // names and no figures — what a piece of work paid is the contractor's
    // business, and the client is often under an agreement Skilluv is not
    // party to.
    let missions: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT mt.slug, count(*)::bigint
          FROM missions m
          JOIN mission_types mt ON mt.id = m.mission_type_id
         WHERE m.assigned_user_id = $1
           AND m.skill_domain = 'design'
           AND m.status IN ('delivered', 'closed')
         GROUP BY mt.slug
         ORDER BY count(*) DESC
        "#,
    )
    .bind(header.id)
    .fetch_all(&state.db)
    .await?;

    // Revealed ratings only, and the average is null rather than zero for
    // somebody with none: an unrated person is not a badly rated one.
    let standing = crate::services::mission_delivery::standing_of(&state.db, header.id).await?;

    // Portfolios on platforms Skilluv does not own. Confirmed rows only: a
    // URL somebody typed is not evidence, and this is a page a recruiter
    // reads.
    let portfolios: Vec<(String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT provider, url, title
          FROM external_signals
         WHERE user_id = $1 AND verified_at IS NOT NULL
         ORDER BY verified_at DESC
        "#,
    )
    .bind(header.id)
    .fetch_all(&state.db)
    .await?;

    // Work that took three rounds or more, with the rounds. This is the one
    // section that shows the process rather than the result, and it is here
    // because a deliverable that survived four critiques says something a
    // finished image cannot.
    //
    // Shown, never ranked. Needing more rounds is not an achievement and not
    // a failure — it is how somebody learned, and a leaderboard of it would
    // teach people to iterate for the leaderboard.
    let stories: Vec<(uuid::Uuid, String, i16, Value)> = sqlx::query_as(
        r#"
        SELECT s.id,
               s.title,
               max(v.round) AS rounds,
               jsonb_agg(
                   jsonb_build_object(
                       'round', v.round,
                       'decision', v.decision,
                       'blocking_reason', v.blocking_reason,
                       'artifact_url', v.reviewed_artifact_url,
                       'decided_at', v.decided_at
                   ) ORDER BY v.round
               ) AS rounds_detail
          FROM countable_deliverables d
          JOIN project_slices s ON s.id = d.slice_id
          JOIN slice_validation_decisions v ON v.slice_id = s.id
         WHERE d.user_id = $1
           AND d.artifact_type = 'design_artifact'
           AND d.public = TRUE
           AND v.decided_at IS NOT NULL
         GROUP BY s.id, s.title
        HAVING max(v.round) >= 3
         ORDER BY max(v.round) DESC
         LIMIT 6
        "#,
    )
    .bind(header.id)
    .fetch_all(&state.db)
    .await?;

    // What they say they are open to. A declaration, never a credential —
    // and separated from everything above it for that reason.
    let availability: Option<(bool, Option<String>)> =
        sqlx::query_as("SELECT available_for_hire, looking_for FROM users WHERE id = $1")
            .bind(header.id)
            .fetch_optional(&state.db)
            .await?;

    Ok(Json(build_response(json!({
        "username": username,
        "craft_score": score,
        "missions": {
            "delivered": missions.iter().map(|(_, n)| n).sum::<i64>(),
            "by_type": missions
                .into_iter()
                .map(|(slug, count)| json!({ "mission_type": slug, "delivered": count }))
                .collect::<Vec<_>>(),
            // Null, not zero, for somebody nobody has rated yet.
            "rating_average": standing.average,
            "rating_count": standing.received,
        },
        "portfolios": portfolios
            .into_iter()
            .map(|(provider, url, title)| json!({
                "provider": provider, "url": url, "title": title,
            }))
            .collect::<Vec<_>>(),
        "iteration_stories": stories
            .into_iter()
            .map(|(slice_id, title, rounds, detail)| json!({
                "slice_id": slice_id, "title": title, "rounds": rounds,
                "timeline": detail,
            }))
            .collect::<Vec<_>>(),
        "availability": availability
            .map(|(available, looking_for)| json!({
                "available_for_missions": available,
                "looking_for": looking_for,
            })),
        "artefacts": artefacts,
        "contests": contests
            .into_iter()
            .map(|(name, rank, entrants)| json!({
                "name": name, "rank": rank, "entrants": entrants,
            }))
            .collect::<Vec<_>>(),
        "trades": trades
            .into_iter()
            .map(|(name, count)| json!({ "trade": name, "validated": count }))
            .collect::<Vec<_>>(),
        "attestations": attestations
            .into_iter()
            .map(|(basis, title, code)| json!({
                "basis": basis, "title": title, "verification_code": code,
            }))
            .collect::<Vec<_>>(),
    }))))
}

/// POST /users/me/design-profile/recompute
#[utoipa::path(
    post, path = "/api/users/me/design-profile/recompute", tag = "design",
    responses((status = 200, description = "the score, recomputed and stored")),
    security(("cookie_auth" = [])),
    operation_id = "designProfileRecomputeMine",
)]
pub async fn recompute_mine(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let score = design_craft_score::recompute(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "craft_score": score }))))
}

/// GET /design/tiers — the ladder, and the formula behind it.
///
/// Published because a score whose rules are private is a ranking nobody can
/// argue with, and the whole point of storing the weights as rows was that
/// somebody should be able to.
#[utoipa::path(
    get, path = "/api/design/tiers", tag = "design",
    responses((status = 200, description = "tiers, weights and the ceiling")),
    operation_id = "designProfileListTiers",
)]
pub async fn list_tiers(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let tiers: Vec<(String, String, i32, Option<i32>, String)> = sqlx::query_as(
        "SELECT slug, name, min_score, max_score, description
           FROM craft_score_tiers WHERE skill_domain = 'design'
          ORDER BY sort_order",
    )
    .fetch_all(&state.db)
    .await?;

    let weights: Vec<(String, bigdecimal::BigDecimal, String, String)> = sqlx::query_as(
        "SELECT term, weight, kind, explanation
           FROM craft_score_weights
          WHERE skill_domain = 'design' AND is_active = TRUE
          ORDER BY sort_order, term",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "cap": design_craft_score::CAP,
        "tiers": tiers
            .into_iter()
            .map(|(slug, name, min_score, max_score, description)| json!({
                "slug": slug, "name": name, "min_score": min_score,
                "max_score": max_score, "description": description,
            }))
            .collect::<Vec<_>>(),
        "weights": weights
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
