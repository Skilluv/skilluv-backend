//! P26 v2 SKI-121 — personalized challenge feed for the authenticated user.
//!
//! `GET /api/me/feed/challenges?limit=20`
//!
//! Returns slices `status='open'` filtered and ordered so a challenger
//! sees challenges they can actually claim + that are close to their
//! current skill level. The generic `/slices` and `/explore` endpoints
//! stay public / catalog-oriented; this one is opinionated for the
//! signed-in user.
//!
//! ─── Filtering rules ─────────────────────────────────────────────
//!
//!   Excluded:
//!     * slices whose `min_rank` is above the user's rank (SKI-78 gate
//!       would refuse the claim — no point showing what they can't take)
//!     * slices whose `required_orientation_slugs` is non-empty AND the
//!       user has no active user_orientation matching (SKI-79 gate)
//!     * slices the user has already claimed before (in the past or now)
//!
//!   Ordered:
//!     * `abs(difficulty - user_median_recent_difficulty)` ascending —
//!       "close to what you usually do"
//!     * then `created_at DESC` — fresher first
//!
//! ─── Fallbacks ────────────────────────────────────────────────────
//!
//! A user with no history has a median of NULL → we fall back to
//! ordering purely by `created_at DESC` (matches the generic feed).

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::ProjectSlice;
use crate::services::slices::rank_ordinal_public;

pub fn challenge_feed_routes() -> Router<AppState> {
    Router::new().route("/me/feed/challenges", get(feed))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct FeedQuery {
    #[serde(default)]
    limit: Option<i64>,
}

/// Challenges picked for the caller, from what they have done and declared.
#[utoipa::path(
    get, path = "/api/me/feed/challenges", tag = "feed",
    params(FeedQuery),
    responses(
        (status = 200, body = serde_json::Value),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn feed(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<FeedQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(20).clamp(1, 50);

    // Get the user's rank ordinal (NULL row → 0 = apprenti).
    let user_rank: Option<(String,)> =
        sqlx::query_as("SELECT rank FROM user_ranks WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?;
    let user_rank_ord = user_rank.map(|(r,)| rank_ordinal_public(&r)).unwrap_or(0);

    // Median difficulty of the user's recent (last 20) claims — used to
    // sort "close to what you usually do". Falls back to 3 (mid) when
    // no history exists.
    let median: Option<f64> = sqlx::query_scalar(
        r#"
        SELECT PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY difficulty::float8)
          FROM (
            SELECT difficulty
              FROM project_slices
             WHERE claimed_by_user_id = $1
             ORDER BY claimed_at DESC NULLS LAST
             LIMIT 20
          ) recent
        "#,
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .flatten();
    let median_difficulty = median.unwrap_or(3.0);

    // The rank filter is expressed with a subquery evaluating the
    // slice's min_rank on the same ordinal — keeps the logic aligned
    // with SKI-78 (`services::slices::assert_rank_access`).
    let rows: Vec<ProjectSlice> = sqlx::query_as::<_, ProjectSlice>(
        r#"
        SELECT s.*
          FROM project_slices s
         WHERE s.status = 'open'
           -- SKI-78 rank gate mirror
           AND (
                s.min_rank IS NULL
             OR CASE s.min_rank
                    WHEN 'apprenti' THEN 0
                    WHEN 'ranger'   THEN 1
                    WHEN 'artisan'  THEN 2
                    WHEN 'maitre'   THEN 3
                    WHEN 'doyen'    THEN 4
                    ELSE 5
                END <= $2::int2
           )
           -- SKI-79 orientation gate mirror
           AND (
                array_length(s.required_orientation_slugs, 1) IS NULL
             OR EXISTS (
                    SELECT 1
                      FROM user_orientations uo
                      JOIN orientations o ON o.id = uo.orientation_id
                     WHERE uo.user_id = $1
                       AND uo.ended_at IS NULL
                       AND o.slug = ANY(s.required_orientation_slugs)
                )
           )
           -- Exclude anything the user has ever touched (past + present)
           AND NOT EXISTS (
                SELECT 1 FROM project_slices past
                 WHERE past.id = s.id
                   AND past.claimed_by_user_id = $1
           )
         ORDER BY abs(s.difficulty::float8 - $3::float8) ASC,
                  s.created_at DESC
         LIMIT $4
        "#,
    )
    .bind(auth.user_id)
    .bind(i16::from(user_rank_ord))
    .bind(median_difficulty)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({
        "data": rows,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "user_rank_ord": user_rank_ord,
            "median_difficulty": median_difficulty,
        }
    })))
}
