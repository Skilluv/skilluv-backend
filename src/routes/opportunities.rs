//! Curated outside opportunities: calls for papers, speaker slots, teaching
//! positions.
//!
//! ## One table for what two backlogs asked for twice
//!
//! Communication's ticket T-02 asked for `external_devrel_opportunities` —
//! conference CFPs and meetup speaker slots. Education's ticket T-03 asked for
//! `external_education_platforms` — bootcamps and coding schools hiring
//! trainers. They are the same three facts: an organisation, a deadline and a
//! link, with a domain saying who it is for.
//!
//! Migration 0513 made them one table for the reasons 0413 and 0415 gave about
//! missions and portfolios: two tables mean two curation flows, two listings,
//! two staleness problems and two answers to "what is open right now".
//!
//! ## Why nothing is seeded and everything is curated
//!
//! A call for papers is true for about three months, and a closed one looks
//! exactly like an open one until somebody applies. Seeding a list in a
//! migration would have shipped a file that was wrong before the first reader
//! saw it. Rows come from a curator, who is accountable for the deadline being
//! right, and `withdrawn_at` takes one down without deleting it — somebody who
//! applied has a right to still find what they applied to.
//!
//! ## Why the default listing hides what has closed
//!
//! `include_closed` exists and defaults to false. A listing that shows expired
//! deadlines by default trains people to ignore the dates, which is the one
//! thing this table has to get right.

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

/// What kinds an opportunity can be. The list is migration 0513's CHECK, and
/// it lives here so a bad request gets the options rather than a constraint
/// name.
const KINDS: &[&str] = &[
    "conference_cfp",
    "meetup_speaker_slot",
    "writing_call",
    "translation_call",
    "teaching_position",
    "curriculum_call",
];

pub fn opportunity_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/opportunities",
            get(list_opportunities).post(create_opportunity),
        )
        .route(
            "/opportunities/{id}",
            axum::routing::delete(withdraw_opportunity),
        )
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct OpportunityQuery {
    /// `communication`, `education`, … Absent means every domain.
    #[param(value_type = Option<crate::validators::SkillDomain>)]
    pub domain: Option<String>,
    /// One of the kinds. Absent means all of them.
    #[param(max_length = 30)]
    pub kind: Option<String>,
    /// Restrict to one trade, by orientation slug. An opportunity aimed at
    /// the whole domain matches every trade in it.
    #[param(max_length = 60)]
    pub orientation: Option<String>,
    /// Remote only.
    pub remote: Option<bool>,
    /// Include the ones whose deadline has passed. False by default: a
    /// listing that shows expired deadlines trains people to ignore dates.
    pub include_closed: Option<bool>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct OpportunityRow {
    pub id: Uuid,
    pub slug: String,
    pub kind: String,
    pub skill_domain: String,
    pub title: String,
    pub organisation: String,
    pub url: String,
    pub summary: String,
    pub location: Option<String>,
    pub country: Option<String>,
    pub is_remote: bool,
    /// When applications close. The one date that has to be right.
    pub closes_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the thing itself happens, where that is a different date.
    pub happens_at: Option<chrono::DateTime<chrono::Utc>>,
    pub orientation_slugs: Vec<String>,
    pub curated_at: chrono::DateTime<chrono::Utc>,
}

/// What is open, soonest deadline first.
///
/// Public and unauthenticated: an opportunity nobody can read without an
/// account is an opportunity that reaches the people who are already here,
/// which is the opposite of the point.
#[utoipa::path(
    get, path = "/api/opportunities", tag = "opportunities",
    params(OpportunityQuery),
    responses((status = 200, description = "Opportunities", body = ApiResponse<Vec<OpportunityRow>>)),
)]
pub async fn list_opportunities(
    State(state): State<AppState>,
    Query(q): Query<OpportunityQuery>,
) -> Result<Json<ApiResponse<Vec<OpportunityRow>>>, AppError> {
    crate::validators::check_skill_domain_opt(&q.domain, "domain")?;
    if let Some(kind) = q.kind.as_deref()
        && !KINDS.contains(&kind)
    {
        return Err(AppError::Validation(format!(
            "kind must be one of: {}",
            KINDS.join(", ")
        )));
    }

    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let include_closed = q.include_closed.unwrap_or(false);

    let rows: Vec<OpportunityRow> = sqlx::query_as(
        r#"
        SELECT id, slug, kind, skill_domain, title, organisation, url, summary,
               location, country, is_remote, closes_at, happens_at,
               orientation_slugs, curated_at
          FROM external_opportunities
         WHERE withdrawn_at IS NULL
           AND ($1::VARCHAR IS NULL OR skill_domain = $1)
           AND ($2::VARCHAR IS NULL OR kind = $2)
           -- An opportunity aimed at the whole domain carries no slugs and
           -- matches every trade in it.
           AND ($3::VARCHAR IS NULL
                OR orientation_slugs = '{}'
                OR $3 = ANY (orientation_slugs))
           AND ($4::BOOLEAN IS NULL OR is_remote = $4)
           -- A row with no deadline never closes, which is the truth about a
           -- standing call.
           AND ($5::BOOLEAN OR closes_at IS NULL OR closes_at > NOW())
         ORDER BY closes_at ASC NULLS LAST, sort_order, curated_at DESC
         LIMIT $6
        "#,
    )
    .bind(q.domain.as_deref())
    .bind(q.kind.as_deref())
    .bind(q.orientation.as_deref())
    .bind(q.remote)
    .bind(include_closed)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateOpportunityBody {
    pub slug: String,
    pub kind: String,
    pub skill_domain: String,
    pub title: String,
    pub organisation: String,
    pub url: String,
    pub summary: Option<String>,
    pub location: Option<String>,
    /// ISO 3166-1 alpha-2.
    pub country: Option<String>,
    pub is_remote: Option<bool>,
    pub closes_at: Option<chrono::DateTime<chrono::Utc>>,
    pub happens_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Trades this is aimed at. Empty means the whole domain.
    pub orientation_slugs: Option<Vec<String>>,
}

/// Put an outside opportunity on the board.
///
/// Restricted to whoever curates the domain, and the row records who. The
/// value of this listing is entirely that somebody checked the deadline, and
/// a deadline nobody is accountable for is worse than no listing.
#[utoipa::path(
    post, path = "/api/opportunities", tag = "opportunities",
    request_body = CreateOpportunityBody,
    responses(
        (status = 200, description = "Curated", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Bad kind, domain or URL", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not a curator of that domain", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_opportunity(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateOpportunityBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    crate::validators::validate_skill_domain(&body.skill_domain, "skill_domain")?;
    if !KINDS.contains(&body.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "kind must be one of: {}",
            KINDS.join(", ")
        )));
    }
    crate::validators::validate_url(&body.url, "url", 1000)?;
    if !body.url.starts_with("https://") {
        return Err(AppError::Validation("url must start with https://".into()));
    }
    crate::validators::check_max_len(&body.title, "title", 200)?;
    crate::validators::check_max_len(&body.organisation, "organisation", 160)?;
    let summary = body.summary.as_deref().unwrap_or("");
    crate::validators::check_max_len(summary, "summary", 4000)?;

    require_curator(&state.db, auth.user_id, &body.skill_domain).await?;

    // Stated rather than left to the CHECK: a constraint name in an error is
    // not something a curator can act on.
    if let (Some(closes), Some(happens)) = (body.closes_at, body.happens_at)
        && happens < closes
    {
        return Err(AppError::Validation(
            "the event cannot happen before applications close".into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO external_opportunities
            (slug, kind, skill_domain, title, organisation, url, summary,
             location, country, is_remote, closes_at, happens_at,
             orientation_slugs, curated_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (slug) DO UPDATE SET
            kind = EXCLUDED.kind,
            title = EXCLUDED.title,
            organisation = EXCLUDED.organisation,
            url = EXCLUDED.url,
            summary = EXCLUDED.summary,
            location = EXCLUDED.location,
            country = EXCLUDED.country,
            is_remote = EXCLUDED.is_remote,
            closes_at = EXCLUDED.closes_at,
            happens_at = EXCLUDED.happens_at,
            orientation_slugs = EXCLUDED.orientation_slugs,
            curated_by = EXCLUDED.curated_by,
            -- Re-curating clears a withdrawal: a call that reopened is the
            -- same call, and a second row would split the applicants.
            withdrawn_at = NULL,
            withdrawn_reason = NULL,
            updated_at = NOW()
        RETURNING id
        "#,
    )
    .bind(body.slug.trim())
    .bind(&body.kind)
    .bind(&body.skill_domain)
    .bind(body.title.trim())
    .bind(body.organisation.trim())
    .bind(body.url.trim())
    .bind(summary.trim())
    .bind(body.location.as_deref())
    .bind(body.country.as_deref())
    .bind(body.is_remote.unwrap_or(false))
    .bind(body.closes_at)
    .bind(body.happens_at)
    .bind(body.orientation_slugs.unwrap_or_default())
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({ "id": id }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WithdrawBody {
    /// Why it came down. Required: "it disappeared" is not something a member
    /// who applied to it can act on.
    pub reason: String,
}

/// Take an opportunity down, with the reason.
///
/// Not a delete. Somebody who applied has a right to still find what they
/// applied to, and a listing that loses rows silently is a listing nobody can
/// audit.
#[utoipa::path(
    delete, path = "/api/opportunities/{id}", tag = "opportunities",
    params(("id" = Uuid, Path, description = "Opportunity")),
    request_body = WithdrawBody,
    responses(
        (status = 200, description = "Withdrawn", body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Not a curator of that domain", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such opportunity", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn withdraw_opportunity(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<WithdrawBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    if body.reason.trim().is_empty() {
        return Err(AppError::Validation(
            "a withdrawal has to say why — a row that disappears is not \
             something an applicant can act on"
                .into(),
        ));
    }
    crate::validators::check_max_len(&body.reason, "reason", 2000)?;

    let domain: Option<String> =
        sqlx::query_scalar("SELECT skill_domain FROM external_opportunities WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let Some(domain) = domain else {
        return Err(AppError::NotFound("opportunity not found".into()));
    };

    require_curator(&state.db, auth.user_id, &domain).await?;

    sqlx::query(
        "UPDATE external_opportunities
            SET withdrawn_at = NOW(), withdrawn_reason = $2, updated_at = NOW()
          WHERE id = $1",
    )
    .bind(id)
    .bind(body.reason.trim())
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({ "withdrawn": true }))))
}

/// Whoever runs this domain, whoever runs all of them, or an admin.
async fn require_curator(db: &sqlx::PgPool, user_id: Uuid, domain: &str) -> Result<(), AppError> {
    let scoped = format!("domain_curator:{domain}");
    crate::middleware::capabilities::require_any_capability(
        db,
        user_id,
        &[&scoped, "domain_curator:all", "admin"],
    )
    .await
}
