//! Running a domain, without running the platform.
//!
//! ## Why these are per-domain and not per-design
//!
//! The ticket asked for `/admin/design/*`. Every figure on those pages —
//! how many people are active, how many challenges are open, how the
//! reviewers are keeping up, who is due a featuring — is the same question
//! asked of a different `skill_domain`. Seven copies would drift, and the
//! sixth would be written by somebody who had forgotten what the first meant.
//!
//! So `/admin/domains/{domain}/…`, and the design admin passes `design`.
//!
//! ## Who may read them
//!
//! An admin, or the curator of that domain. `domain_curator:design` is
//! somebody who publishes design challenges, opens design contests and
//! schedules design featurings — and who cannot ban anybody, move any money
//! or read the financial dashboard. Until now that job required `admin`,
//! which grants all three.
//!
//! `domain_curator:all` exists for the person who curates everything, so that
//! job does not have to be spelled as seven grants that fall out of sync when
//! a domain is added.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn admin_domain_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/domains/{domain}/overview", get(overview))
        .route("/admin/domains/{domain}/reviewers", get(reviewers))
        .route(
            "/admin/domains/{domain}/featured-queue",
            get(featured_queue),
        )
}

/// An admin, or the curator of this domain, or the curator of all of them.
///
/// Read separately from the write paths on purpose: seeing how a domain is
/// doing is not the same permission as changing it, and a curator who can
/// read the reviewer backlog without being able to revoke a capability is a
/// useful person to have.
async fn require_curator(state: &AppState, auth: &AuthUser, domain: &str) -> Result<(), AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &[
            "admin",
            &format!("domain_curator:{domain}"),
            "domain_curator:all",
        ],
    )
    .await
}

// ═══════════════════════════════════════════════════════════════════
// Overview
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct WindowQuery {
    /// How far back the activity figures look. Thirty days by default: short
    /// enough that a quiet fortnight shows, long enough that one holiday
    /// week does not read as a collapse.
    #[serde(default = "default_days")]
    #[param(minimum = 1, maximum = 365)]
    pub days: i32,
}

fn default_days() -> i32 {
    30
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DomainOverview {
    pub skill_domain: String,
    pub window_days: i32,

    /// People who declare a trade in this domain and have not left it.
    pub declared_trades: i64,
    /// Of those, how many have done something inside the window. The gap
    /// between the two is the figure worth watching: a domain with two
    /// hundred declarations and four active people has a problem no total
    /// will show.
    pub active_contributors: i64,

    pub challenges_published: i64,
    pub challenges_draft: i64,

    pub contests_running: i64,
    pub contests_concluded_in_window: i64,

    pub missions_in_progress: i64,
    pub missions_delivered_in_window: i64,

    /// Slices waiting for a reviewer to pick them up.
    pub reviews_pending: i64,
    /// The oldest of them, in hours. A queue's length says how much work
    /// there is; its age says whether anybody is doing it.
    pub oldest_pending_review_hours: Option<f64>,
    /// Mean rounds to an approval, over deliverables approved in the window.
    /// Rounds are how somebody learns, so this is a health figure and not a
    /// target — it is meant to be read next to the approval rate, not alone.
    pub mean_rounds_to_approval: Option<f64>,

    /// The Monday of the most recent featuring in this domain, if any.
    pub last_featured_week: Option<chrono::NaiveDate>,
}

/// How a domain is doing.
#[utoipa::path(
    get,
    path = "/api/admin/domains/{domain}/overview",
    tag = "admin",
    params(("domain" = String, Path, description = "One of the seven skill domains"), WindowQuery),
    responses(
        (status = 200, body = ApiResponse<DomainOverview>),
        (status = 400, description = "Unknown domain", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin or a curator of this domain", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn overview(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
    Query(q): Query<WindowQuery>,
) -> Result<Json<ApiResponse<DomainOverview>>, AppError> {
    crate::validators::validate_skill_domain(&domain, "domain")?;
    if !(1..=365).contains(&q.days) {
        return Err(AppError::Validation(
            "days must be between 1 and 365".into(),
        ));
    }
    require_curator(&state, &auth, &domain).await?;

    // One round trip. Eleven counters as eleven queries would be eleven
    // chances for the page to show figures from eleven different instants.
    let row = sqlx::query(
        r#"
        WITH win AS (SELECT NOW() - ($2::INTEGER * INTERVAL '1 day') AS since),
        people AS (
            SELECT DISTINCT uo.user_id
              FROM user_orientations uo
              JOIN orientations o ON o.id = uo.orientation_id
             WHERE uo.ended_at IS NULL AND o.primary_domain = $1
        )
        SELECT
            (SELECT count(*) FROM people) AS declared_trades,

            (SELECT count(DISTINCT p.user_id)
               FROM people p
               JOIN project_slices ps ON ps.claimed_by_user_id = p.user_id
              WHERE ps.primary_domain = $1
                AND ps.updated_at > (SELECT since FROM win)) AS active_contributors,

            (SELECT count(*) FROM challenge_templates
              WHERE skill_domain = $1 AND status = 'published') AS challenges_published,
            (SELECT count(*) FROM challenge_templates
              WHERE skill_domain = $1 AND status = 'draft') AS challenges_draft,

            (SELECT count(*) FROM tournaments
              WHERE skill_domain = $1
                AND status IN ('registration', 'active')) AS contests_running,
            (SELECT count(*) FROM tournaments
              WHERE skill_domain = $1 AND status = 'concluded'
                AND updated_at > (SELECT since FROM win)) AS contests_concluded_in_window,

            (SELECT count(*) FROM missions
              WHERE skill_domain = $1 AND status = 'in_progress') AS missions_in_progress,
            (SELECT count(*) FROM missions
              WHERE skill_domain = $1 AND status IN ('delivered', 'closed')
                AND updated_at > (SELECT since FROM win)) AS missions_delivered_in_window,

            -- Picked up but not decided. A row with `decided_at` set is a
            -- round that is over, whatever it decided.
            (SELECT count(*)
               FROM slice_validation_decisions svd
               JOIN project_slices ps2 ON ps2.id = svd.slice_id
              WHERE ps2.primary_domain = $1
                AND svd.decided_at IS NULL) AS reviews_pending,

            (SELECT (EXTRACT(EPOCH FROM (NOW() - min(svd2.picked_at))) / 3600.0)::FLOAT8
               FROM slice_validation_decisions svd2
               JOIN project_slices ps3 ON ps3.id = svd2.slice_id
              WHERE ps3.primary_domain = $1
                AND svd2.decided_at IS NULL) AS oldest_pending_review_hours,

            (SELECT avg(svd3.round)::FLOAT8
               FROM slice_validation_decisions svd3
               JOIN project_slices ps4 ON ps4.id = svd3.slice_id
              WHERE ps4.primary_domain = $1
                AND svd3.decision = 'approve'
                AND svd3.decided_at > (SELECT since FROM win)) AS mean_rounds_to_approval,

            (SELECT max(week_of) FROM featured_talents
              WHERE skill_domain = $1) AS last_featured_week
        "#,
    )
    .bind(&domain)
    .bind(q.days)
    .fetch_one(&state.db)
    .await?;

    use sqlx::Row;
    Ok(Json(ApiResponse::new(DomainOverview {
        skill_domain: domain,
        window_days: q.days,
        declared_trades: row.get("declared_trades"),
        active_contributors: row.get("active_contributors"),
        challenges_published: row.get("challenges_published"),
        challenges_draft: row.get("challenges_draft"),
        contests_running: row.get("contests_running"),
        contests_concluded_in_window: row.get("contests_concluded_in_window"),
        missions_in_progress: row.get("missions_in_progress"),
        missions_delivered_in_window: row.get("missions_delivered_in_window"),
        reviews_pending: row.get("reviews_pending"),
        oldest_pending_review_hours: row.get("oldest_pending_review_hours"),
        mean_rounds_to_approval: row.get("mean_rounds_to_approval"),
        last_featured_week: row.get("last_featured_week"),
    })))
}

// ═══════════════════════════════════════════════════════════════════
// Reviewers
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct ReviewerStats {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    /// Every review capability they hold in this domain, so a curator can see
    /// at a glance who is spread across five families and who covers one.
    pub families: Vec<String>,
    pub decisions_total: i64,
    pub approved: i64,
    pub iterations_asked: i64,
    pub rejected: i64,
    /// Mean hours between picking a slice up and deciding on it. Null for
    /// somebody who has decided nothing yet, rather than zero — never having
    /// reviewed is not reviewing instantly.
    pub mean_hours_to_decide: Option<f64>,
    /// Still picked up, still undecided. The figure that says a reviewer has
    /// taken on more than they are getting through.
    pub open_now: i64,
}

/// Who reviews in this domain, and how they are keeping up.
///
/// No ranking and no target. A reviewer who asks for iterations more often
/// than the others is not necessarily doing it wrong — the whole point of the
/// round is that saying "not yet" is a valid answer, and a page that shames
/// it would teach reviewers to approve.
#[utoipa::path(
    get,
    path = "/api/admin/domains/{domain}/reviewers",
    tag = "admin",
    params(("domain" = String, Path, description = "One of the seven skill domains"), WindowQuery),
    responses(
        (status = 200, body = ApiResponse<Vec<ReviewerStats>>),
        (status = 400, description = "Unknown domain", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin or a curator of this domain", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn reviewers(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
    Query(q): Query<WindowQuery>,
) -> Result<Json<ApiResponse<Vec<ReviewerStats>>>, AppError> {
    crate::validators::validate_skill_domain(&domain, "domain")?;
    if !(1..=365).contains(&q.days) {
        return Err(AppError::Validation(
            "days must be between 1 and 365".into(),
        ));
    }
    require_curator(&state, &auth, &domain).await?;

    // `design_reviewer:brand` → the prefix is the domain, the scope after the
    // colon is the family. The same shape holds for code and AI, which is why
    // this endpoint does not care which domain it was asked about.
    let prefix = format!("{domain}_reviewer:");

    let rows = sqlx::query_as::<_, ReviewerStats>(
        r#"
        SELECT u.id AS user_id,
               u.username,
               u.display_name,
               array_agg(DISTINCT split_part(uc.capability, ':', 2)) AS families,
               COALESCE(d.total, 0) AS decisions_total,
               COALESCE(d.approved, 0) AS approved,
               COALESCE(d.iterations_asked, 0) AS iterations_asked,
               COALESCE(d.rejected, 0) AS rejected,
               d.mean_hours_to_decide,
               COALESCE(o.open_now, 0) AS open_now
          FROM user_capabilities uc
          JOIN users u ON u.id = uc.user_id

          LEFT JOIN LATERAL (
              SELECT count(*) AS total,
                     count(*) FILTER (WHERE svd.decision = 'approve') AS approved,
                     count(*) FILTER (WHERE svd.decision = 'iterate') AS iterations_asked,
                     count(*) FILTER (WHERE svd.decision = 'reject') AS rejected,
                     avg(EXTRACT(EPOCH FROM (svd.decided_at - svd.picked_at)) / 3600.0)::FLOAT8
                         AS mean_hours_to_decide
                FROM slice_validation_decisions svd
               WHERE svd.validator_id = u.id
                 AND svd.decided_at IS NOT NULL
                 AND svd.decided_at > NOW() - ($2::INTEGER * INTERVAL '1 day')
          ) AS d ON TRUE

          -- Deliberately not windowed: a slice picked up two months ago and
          -- never decided is exactly what a curator needs to see, and a
          -- thirty-day window would hide it.
          LEFT JOIN LATERAL (
              SELECT count(*) AS open_now
                FROM slice_validation_decisions svd2
               WHERE svd2.validator_id = u.id AND svd2.decided_at IS NULL
          ) AS o ON TRUE

         WHERE uc.capability LIKE $1 || '%'
           AND uc.revoked_at IS NULL
           AND (uc.expires_at IS NULL OR uc.expires_at > NOW())

         GROUP BY u.id, u.username, u.display_name, d.total, d.approved,
                  d.iterations_asked, d.rejected, d.mean_hours_to_decide, o.open_now
         ORDER BY COALESCE(o.open_now, 0) DESC, COALESCE(d.total, 0) DESC, u.username
        "#,
    )
    .bind(&prefix)
    .bind(q.days)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}

// ═══════════════════════════════════════════════════════════════════
// Who is due a featuring
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct FeaturedCandidate {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub craft_score: i32,
    /// Deliverables approved inside the window. The reason to feature
    /// somebody is work, so this is what the list is ordered by — not the
    /// score, which is cumulative and would return the same five names every
    /// week until one of them died.
    pub approved_in_window: i64,
    /// When they were last featured, if ever. Somebody featured recently is
    /// still listed rather than filtered out: a curator deciding to feature
    /// somebody twice should do it knowingly, not be prevented.
    pub last_featured_on: Option<chrono::NaiveDate>,
}

/// Who a curator might feature this week.
///
/// A suggestion, never a decision. The featuring itself is
/// `POST /api/admin/featured`, it requires a written reason of at least forty
/// characters, and the attestation it produces says in as many words that it
/// rests on somebody's judgement.
#[utoipa::path(
    get,
    path = "/api/admin/domains/{domain}/featured-queue",
    tag = "admin",
    params(("domain" = String, Path, description = "One of the seven skill domains"), WindowQuery),
    responses(
        (status = 200, body = ApiResponse<Vec<FeaturedCandidate>>),
        (status = 400, description = "Unknown domain", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin or a curator of this domain", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn featured_queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
    Query(q): Query<WindowQuery>,
) -> Result<Json<ApiResponse<Vec<FeaturedCandidate>>>, AppError> {
    crate::validators::validate_skill_domain(&domain, "domain")?;
    if !(1..=365).contains(&q.days) {
        return Err(AppError::Validation(
            "days must be between 1 and 365".into(),
        ));
    }
    require_curator(&state, &auth, &domain).await?;

    let rows = sqlx::query_as::<_, FeaturedCandidate>(
        r#"
        SELECT u.id AS user_id,
               u.username,
               u.display_name,
               COALESCE(cs.score, 0) AS craft_score,
               count(*) AS approved_in_window,
               (SELECT max(ft.week_of) FROM featured_talents ft
                 WHERE ft.user_id = u.id) AS last_featured_on
          FROM slice_validation_decisions svd
          JOIN project_slices ps ON ps.id = svd.slice_id
          JOIN users u ON u.id = ps.claimed_by_user_id
          LEFT JOIN craft_scores cs ON cs.user_id = u.id AND cs.skill_domain = $1
         WHERE ps.primary_domain = $1
           AND svd.decision = 'approve'
           AND svd.decided_at > NOW() - ($2::INTEGER * INTERVAL '1 day')
           AND u.profile_active = TRUE
           AND u.is_banned = FALSE
         GROUP BY u.id, u.username, u.display_name, cs.score
         ORDER BY count(*) DESC, COALESCE(cs.score, 0) DESC, u.username
         LIMIT 20
        "#,
    )
    .bind(&domain)
    .bind(q.days)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}
