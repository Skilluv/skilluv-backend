//! Where somebody practises a trade, and what they practise with.
//!
//! Two listings, both keyed on the domain rather than written once per
//! domain:
//!
//!   * `GET /api/domains/{domain}/toolkit`  — the curated tools, courses and
//!     communities, each with what it costs to reach.
//!   * `GET /api/domains/{domain}/terrains` — upstream projects somebody
//!     researched as good places to contribute, and whether one has a steward.
//!
//! ## Why these were not reachable before
//!
//! `external_resources` had two endpoints, `/ai/toolkit` and `/ops/toolkit`,
//! each with its domain written into the SQL. Six other domains had rows in
//! that table and no way to read them. `terrain_proposals` — twenty rows
//! across three domains — had no endpoint at all: the seed migrations were
//! written, the listing never was, and nothing failed because nothing looked.
//!
//! ## Adoption is a decision, not a listing
//!
//! A terrain proposal is a shortlist entry. It becomes a real terrain when a
//! steward takes it, and `adopted_project_id` is where that is recorded. The
//! two write endpoints here are the curator's: adopt, pointing at the project
//! whose owner has agreed to steward it, or decline with the reason, so the
//! next person researching the domain does not propose it again.
//!
//! Declining writes a reason because a shortlist that silently loses entries
//! teaches nobody anything. Adopting requires an existing project rather than
//! creating one, for the reason migration 0418 gives: a project has an owner
//! who answers for it, and no endpoint can appoint that person.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::middleware::capabilities;

/// Fifteen minutes. The list changes when somebody curates it, which is a few
/// times a year, and the cost of being fifteen minutes stale is nothing.
const TOOLKIT_CACHE_TTL_SECS: u64 = 900;

pub fn practice_routes() -> Router<AppState> {
    Router::new()
        .route("/domains/{domain}/toolkit", get(toolkit))
        .route("/domains/{domain}/terrains", get(terrains))
        .route("/domains/{domain}/terrains/{slug}/adopt", post(adopt))
        .route("/domains/{domain}/terrains/{slug}/decline", post(decline))
}

/// The domain has to exist before anything is looked up under it.
///
/// Against `skill_domains` rather than a list in this file: the catalogue is
/// the table since migration 0400, and a second list here would be a second
/// thing to update when a domain opens.
async fn known_domain(db: &sqlx::PgPool, domain: &str) -> Result<(), AppError> {
    crate::validators::check_max_len(domain, "domain", 30)?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM skill_domains WHERE slug = $1)")
            .bind(domain)
            .fetch_one(db)
            .await?;
    if !exists {
        return Err(AppError::NotFound(format!("no domain `{domain}`")));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// GET /domains/{domain}/toolkit
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
pub struct ToolkitQuery {
    /// Narrow to one category. The categories are rows in
    /// `external_resource_categories`, not a list this endpoint holds.
    pub category: Option<String>,
    /// Narrow to the trades one orientation covers.
    pub orientation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct ToolkitRow {
    pub slug: String,
    pub display_name: String,
    pub category: String,
    pub url: String,
    pub summary: String,
    /// What it costs to actually reach it. The half of the answer no upstream
    /// list writes down, and the half that decides whether somebody with no
    /// budget can start at all.
    pub access_note: String,
    pub orientation_slugs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ToolkitResponse {
    pub domain: String,
    pub resources: Vec<ToolkitRow>,
    pub category: Option<String>,
    pub orientation: Option<String>,
}

/// The curated toolkit for one domain, with what each entry costs to reach.
///
/// The `access_note` on every row is the point of the list. A page that names
/// Terraform, Figma and Datadog without saying what each one costs to reach is
/// a page that tells somebody in Cotonou the trade is not for them.
#[utoipa::path(
    get, path = "/api/domains/{domain}/toolkit", tag = "public",
    params(
        ("domain" = String, Path, description = "Which domain's toolkit"),
        ToolkitQuery,
    ),
    responses(
        (status = 200, body = ApiResponse<ToolkitResponse>),
        (status = 404, description = "No such domain", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn toolkit(
    State(state): State<AppState>,
    Path(domain): Path<String>,
    Query(q): Query<ToolkitQuery>,
) -> Result<Json<ApiResponse<ToolkitResponse>>, AppError> {
    known_domain(&state.db, &domain).await?;
    crate::validators::check_max_len_opt(&q.category, "category", 30)?;
    crate::validators::check_max_len_opt(&q.orientation, "orientation", 60)?;

    // Namespaced by database: a Redis instance shared between staging and
    // production would otherwise serve one's listing to the other.
    let cache_key = format!(
        "toolkit:{}:{}:{}:{}",
        state.db.connect_options().get_database().unwrap_or("db"),
        domain,
        q.category.as_deref().unwrap_or("-"),
        q.orientation.as_deref().unwrap_or("-"),
    );
    let mut redis = state.redis.clone();
    if let Some(cached) =
        crate::services::cache::get_json::<ToolkitResponse>(&mut redis, &cache_key).await?
    {
        return Ok(Json(ApiResponse::new(cached)));
    }

    let resources = sqlx::query_as::<_, ToolkitRow>(
        r#"
        SELECT slug, display_name, category, url, summary, access_note,
               orientation_slugs
          FROM external_resources
         WHERE is_curated = TRUE
           AND domain = $1
           AND ($2::TEXT IS NULL OR category = $2)
           -- A resource tagged for no trade serves every trade, so it stays
           -- in a filtered listing. Excluding it would hide HuggingFace from
           -- somebody who asked for the NLP toolkit.
           AND ($3::TEXT IS NULL
                OR cardinality(orientation_slugs) = 0
                OR $3 = ANY(orientation_slugs))
         ORDER BY category, sort_order, display_name
        "#,
    )
    .bind(&domain)
    .bind(q.category.as_deref())
    .bind(q.orientation.as_deref())
    .fetch_all(&state.db)
    .await?;

    let response = ToolkitResponse {
        domain,
        resources,
        category: q.category.clone(),
        orientation: q.orientation.clone(),
    };
    let _ =
        crate::services::cache::set_json(&mut redis, &cache_key, &response, TOOLKIT_CACHE_TTL_SECS)
            .await;

    Ok(Json(ApiResponse::new(response)))
}

// ═══════════════════════════════════════════════════════════════════
// GET /domains/{domain}/terrains
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
pub struct TerrainQuery {
    /// Include the ones a curator turned down, with the reason. Off by
    /// default, on for anybody researching the domain who wants to know what
    /// has already been considered.
    #[serde(default)]
    pub include_declined: bool,
}

/// Where the work of a domain can actually be done.
///
/// A shortlist somebody researched, not a claim that any of it is staffed.
/// `adopted_project_id` is null until a steward takes one on, and that is the
/// moment the terrain becomes real — until then the honest answer is "here is
/// what looks promising and why".
#[utoipa::path(
    get, path = "/api/domains/{domain}/terrains", tag = "public",
    params(
        ("domain" = String, Path, description = "Which domain's terrains"),
        TerrainQuery,
    ),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such domain", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn terrains(
    State(state): State<AppState>,
    Path(domain): Path<String>,
    Query(q): Query<TerrainQuery>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    known_domain(&state.db, &domain).await?;

    let rows: Vec<Value> = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'slug', t.slug, 'name', t.name, 'kind', t.kind,
                   'upstream_url', t.upstream_url,
                   'ingestion_labels', t.ingestion_labels,
                   'why_md', t.why_md,
                   'adopted', t.adopted_project_id IS NOT NULL,
                   'adopted_at', t.adopted_at,
                   'project_slug', p.slug,
                   'declined_at', t.declined_at,
                   'declined_reason', t.declined_reason)
          FROM terrain_proposals t
          LEFT JOIN projects p ON p.id = t.adopted_project_id
         WHERE t.skill_domain = $1
           AND ($2 OR t.declined_at IS NULL)
         ORDER BY (t.adopted_project_id IS NULL), t.sort_order, t.name
        "#,
    )
    .bind(&domain)
    .bind(q.include_declined)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({
        "domain": domain,
        "terrains": rows,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Curating the shortlist
// ═══════════════════════════════════════════════════════════════════

/// Whoever runs the domain decides what its terrains are.
///
/// `domain_curator:{domain}`, the capability migration 0404 derives, rather
/// than admin: which upstream projects welcome this trade's contributions is
/// a judgement about the trade, and the person who has it is not usually the
/// person with the admin flag.
async fn require_curator(db: &sqlx::PgPool, user_id: Uuid, domain: &str) -> Result<(), AppError> {
    capabilities::require_capability(db, user_id, &format!("domain_curator:{domain}")).await
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AdoptBody {
    /// The project that will carry this terrain. It has to exist: a project
    /// has an owner who greets newcomers and answers for what happens there,
    /// and no endpoint can appoint that person.
    pub project_slug: String,
}

/// Record that a steward has taken a proposed terrain on.
#[utoipa::path(
    post, path = "/api/domains/{domain}/terrains/{slug}/adopt", tag = "moderation",
    params(
        ("domain" = String, Path, description = "Which domain"),
        ("slug" = String, Path, description = "The proposal"),
    ),
    request_body = AdoptBody,
    responses(
        (status = 200, description = "Adopted, and now a real terrain"),
        (status = 403, description = "Not this domain's curator", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such proposal, or no such project", body = crate::api_response::ErrorResponse),
        (status = 409, description = "Already adopted, or already declined", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn adopt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((domain, slug)): Path<(String, String)>,
    Json(body): Json<AdoptBody>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    known_domain(&state.db, &domain).await?;
    require_curator(&state.db, auth.user_id, &domain).await?;
    crate::validators::check_max_len(&body.project_slug, "project_slug", 100)?;

    let project_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM projects WHERE slug = $1")
        .bind(&body.project_slug)
        .fetch_optional(&state.db)
        .await?;
    let project_id = project_id.ok_or_else(|| {
        AppError::NotFound(format!(
            "no project `{}` — adopt a terrain onto a project somebody owns, \
             because owning it is what adoption means",
            body.project_slug
        ))
    })?;

    let updated = sqlx::query(
        "UPDATE terrain_proposals
            SET adopted_project_id = $3, adopted_at = NOW()
          WHERE slug = $1 AND skill_domain = $2
            AND adopted_project_id IS NULL AND declined_at IS NULL",
    )
    .bind(&slug)
    .bind(&domain)
    .bind(project_id)
    .execute(&state.db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(already_settled(&state.db, &domain, &slug).await);
    }

    Ok(Json(ApiResponse::new(json!({
        "slug": slug,
        "adopted_by_project": body.project_slug,
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeclineBody {
    /// Why it is not a terrain after all. Required, because a shortlist that
    /// silently loses entries teaches the next researcher nothing and they
    /// will propose the same project again.
    pub reason: String,
}

/// Turn a proposed terrain down, on the record.
#[utoipa::path(
    post, path = "/api/domains/{domain}/terrains/{slug}/decline", tag = "moderation",
    params(
        ("domain" = String, Path, description = "Which domain"),
        ("slug" = String, Path, description = "The proposal"),
    ),
    request_body = DeclineBody,
    responses(
        (status = 200, description = "Declined, with the reason on the record"),
        (status = 403, description = "Not this domain's curator", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such proposal", body = crate::api_response::ErrorResponse),
        (status = 409, description = "Already adopted, or already declined", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn decline(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((domain, slug)): Path<(String, String)>,
    Json(body): Json<DeclineBody>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    known_domain(&state.db, &domain).await?;
    require_curator(&state.db, auth.user_id, &domain).await?;

    let reason = body.reason.trim();
    if reason.is_empty() {
        return Err(AppError::Validation(
            "say why — a proposal turned down without a reason gets proposed again".into(),
        ));
    }
    crate::validators::check_max_len(reason, "reason", 500)?;

    let updated = sqlx::query(
        "UPDATE terrain_proposals
            SET declined_at = NOW(), declined_reason = $3
          WHERE slug = $1 AND skill_domain = $2
            AND adopted_project_id IS NULL AND declined_at IS NULL",
    )
    .bind(&slug)
    .bind(&domain)
    .bind(reason)
    .execute(&state.db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(already_settled(&state.db, &domain, &slug).await);
    }

    Ok(Json(ApiResponse::new(json!({
        "slug": slug,
        "declined_reason": reason,
    }))))
}

/// Say which of the two things went wrong.
///
/// An update that changed nothing means either there is no such proposal or
/// somebody settled it first, and answering 404 for both would send a curator
/// looking for a row that is sitting right there.
async fn already_settled(db: &sqlx::PgPool, domain: &str, slug: &str) -> AppError {
    let state: Option<(bool, bool)> = sqlx::query_as(
        "SELECT adopted_project_id IS NOT NULL, declined_at IS NOT NULL
           FROM terrain_proposals WHERE slug = $1 AND skill_domain = $2",
    )
    .bind(slug)
    .bind(domain)
    .fetch_optional(db)
    .await
    .unwrap_or(None);

    match state {
        None => AppError::NotFound(format!("no terrain proposal `{slug}` in `{domain}`")),
        Some((true, _)) => AppError::Conflict(format!("`{slug}` has already been adopted")),
        Some((_, true)) => AppError::Conflict(format!("`{slug}` has already been declined")),
        Some(_) => AppError::Conflict(format!("`{slug}` could not be settled")),
    }
}
