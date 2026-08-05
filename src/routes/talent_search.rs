use axum::extract::{Path, Query, State};
use axum::http::request::Parts;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::{ApiResponse, MetaInfo};
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::routes::notifications::Pagination;
use crate::services::AuthService;

pub fn talent_search_routes() -> Router<AppState> {
    Router::new()
        .route("/talents/search", get(search_talents))
        .route("/talents/{username}/card", get(talent_card))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct SearchQuery {
    /// Free-text query (FTS on `search_vector`).
    #[param(max_length = 200)]
    pub q: Option<String>,
    #[param(pattern = r"^(code|design|game|security)$")]
    pub skill_domain: Option<String>,
    #[param(max_length = 100)]
    pub title: Option<String>,
    #[param(pattern = r"^[A-Z]{2}$")]
    pub country: Option<String>,
    #[param(minimum = 0, maximum = 1000000)]
    pub min_fragments: Option<i32>,
    /// `fragments` (default), `recent`, `relevance`.
    #[param(pattern = r"^(fragments|recent|relevance)$")]
    pub sort_by: Option<String>,
    #[param(minimum = 1, maximum = 100000)]
    pub page: Option<i64>,
    #[param(minimum = 1, maximum = 100)]
    pub per_page: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct TalentResult {
    id: Uuid,
    username: String,
    display_name: String,
    skill_domain: String,
    title: String,
    golden_stars: i32,
    total_fragments: i32,
    streak_current: i32,
    country: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Search-result entry. `is_bookmarked` is only populated for
/// authenticated enterprise callers.
#[derive(Debug, Serialize, ToSchema)]
pub struct TalentSearchEntry {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub skill_domain: String,
    pub title: String,
    pub golden_stars: i32,
    pub total_fragments: i32,
    pub streak_current: i32,
    pub country: Option<String>,
    /// RFC 3339 timestamp of account creation.
    pub member_since: String,
    /// Present only when the caller is an active enterprise member.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_bookmarked: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TalentSearchResponse {
    pub data: Vec<TalentSearchEntry>,
    pub pagination: Pagination,
    pub meta: MetaInfo,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TalentCardTopSkill {
    pub domain: String,
    pub sub_skill: String,
    pub fragments: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TalentCardResponse {
    pub username: String,
    pub display_name: String,
    pub skill_domain: String,
    pub title: String,
    pub golden_stars: i32,
    pub total_fragments: i32,
    pub streak_current: i32,
    pub country: Option<String>,
    pub member_since: String,
    pub top_skills: Vec<TalentCardTopSkill>,
    pub badge_count: i64,
}

/// Try to extract auth from cookies without failing if absent.
fn try_extract_auth(parts: &Parts, state: &AppState) -> Option<AuthUser> {
    let cookie_header = parts.headers.get("cookie").and_then(|v| v.to_str().ok())?;

    let token = cookie_header
        .split(';')
        .map(|s| s.trim())
        .find(|s| s.starts_with("access_token="))
        .and_then(|s| s.strip_prefix("access_token="))?;

    let claims = AuthService::verify_access_token(token, &state.config.jwt_secret).ok()?;
    let user_id = claims.sub.parse::<Uuid>().ok()?;
    let active_enterprise_id = cookie_header
        .split(';')
        .map(|s| s.trim())
        .find(|s| s.starts_with("active_enterprise="))
        .and_then(|s| s.strip_prefix("active_enterprise="))
        .and_then(|v| Uuid::parse_str(v).ok());
    Some(AuthUser {
        user_id,
        role: claims.role,
        login_method: claims
            .login_method
            .unwrap_or_else(|| "password".to_string()),
        active_enterprise_id,
    })
}

/// Paginated talent search — no auth required (SSR-ready). Authenticated
/// enterprise callers get an extra `is_bookmarked` flag per entry.
#[utoipa::path(
    get,
    path = "/api/talents/search",
    tag = "enterprise",
    params(SearchQuery),
    responses(
        (status = 200, description = "Paginated talents", body = TalentSearchResponse),
    ),
)]
pub async fn search_talents(
    State(state): State<AppState>,
    parts: Parts,
    Query(query): Query<SearchQuery>,
) -> Result<Json<TalentSearchResponse>, AppError> {
    crate::validators::check_max_len_opt(&query.q, "q", 200)?;
    crate::validators::check_max_len_opt(&query.title, "title", 100)?;
    crate::validators::check_range_opt(
        query.min_fragments.map(i64::from),
        "min_fragments",
        0,
        1_000_000,
    )?;
    crate::validators::check_range_opt(query.page, "page", 1, 100_000)?;
    crate::validators::check_range_opt(query.per_page, "per_page", 1, 100)?;
    if let Some(s) = &query.sort_by
        && !matches!(s.as_str(), "fragments" | "recent" | "relevance")
    {
        return Err(AppError::Validation(
            "sort_by must be one of: fragments, recent, relevance".into(),
        ));
    }
    if let Some(d) = &query.skill_domain
        && !matches!(d.as_str(), "code" | "design" | "game" | "security")
    {
        return Err(AppError::Validation(
            "skill_domain must be one of: code, design, game, security".into(),
        ));
    }
    if let Some(c) = &query.country
        && !(c.len() == 2 && c.chars().all(|c| c.is_ascii_uppercase()))
    {
        return Err(AppError::Validation(
            "country must be ISO 3166-1 alpha-2".into(),
        ));
    }
    let auth = try_extract_auth(&parts, &state);
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;
    let sort_by = query.sort_by.as_deref().unwrap_or("fragments");

    // Build dynamic query
    let mut param_idx = 0u32;
    let mut where_clauses = vec![];

    if query.skill_domain.is_some() {
        param_idx += 1;
        where_clauses.push(format!("u.skill_domain = ${param_idx}"));
    }
    if query.title.is_some() {
        param_idx += 1;
        where_clauses.push(format!("u.title = ${param_idx}"));
    }
    if query.country.is_some() {
        param_idx += 1;
        where_clauses.push(format!("u.country = ${param_idx}"));
    }
    if query.min_fragments.is_some() {
        param_idx += 1;
        where_clauses.push(format!("u.total_fragments >= ${param_idx}"));
    }
    if query.q.is_some() {
        param_idx += 1;
        where_clauses.push(format!(
            "u.search_vector @@ to_tsquery('simple', ${param_idx})"
        ));
    }

    let base_where =
        "u.role = 'user' AND u.profile_active = TRUE AND u.is_banned = FALSE".to_string();
    let extra_where = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" AND {}", where_clauses.join(" AND "))
    };

    let order_by = match sort_by {
        "recent" => "u.updated_at DESC",
        "relevance" if query.q.is_some() => "u.total_fragments DESC", // simplified — FTS rank is complex with dynamic binding
        _ => "u.total_fragments DESC",
    };

    let sql = format!(
        "SELECT u.id, u.username, u.display_name, u.skill_domain, u.title, u.golden_stars, u.total_fragments, u.streak_current, u.country, u.created_at FROM users u WHERE {base_where}{extra_where} ORDER BY {order_by} LIMIT {per_page} OFFSET {offset}"
    );

    let count_sql = format!("SELECT COUNT(*) FROM users u WHERE {base_where}{extra_where}");

    // Build queries with dynamic bindings
    let mut db_query = sqlx::query_as::<_, TalentResult>(sqlx::AssertSqlSafe(sql.as_str()));
    let mut count_query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql.as_str()));

    if let Some(ref domain) = query.skill_domain {
        db_query = db_query.bind(domain);
        count_query = count_query.bind(domain);
    }
    if let Some(ref title) = query.title {
        db_query = db_query.bind(title);
        count_query = count_query.bind(title);
    }
    if let Some(ref country) = query.country {
        db_query = db_query.bind(country);
        count_query = count_query.bind(country);
    }
    if let Some(min_frags) = query.min_fragments {
        db_query = db_query.bind(min_frags);
        count_query = count_query.bind(min_frags);
    }
    if let Some(ref q) = query.q {
        // Convert search term to tsquery format
        let tsquery = q.split_whitespace().collect::<Vec<_>>().join(" & ");
        db_query = db_query.bind(tsquery.clone());
        count_query = count_query.bind(tsquery);
    }

    let talents: Vec<TalentResult> = db_query.fetch_all(&state.db).await?;
    let total: i64 = count_query.fetch_one(&state.db).await?;

    // If enterprise user, check bookmarks
    let mut enterprise_id: Option<Uuid> = None;
    let mut bookmarked_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

    if let Some(ref auth) = auth {
        let eid: Option<(Uuid,)> = sqlx::query_as(
            "SELECT e.id FROM enterprises e JOIN enterprise_members em ON em.enterprise_id = e.id WHERE em.user_id = $1 AND em.status = 'active'",
        )
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await?;

        if let Some((eid,)) = eid {
            enterprise_id = Some(eid);
            let talent_ids: Vec<Uuid> = talents.iter().map(|t| t.id).collect();
            let bookmarks: Vec<(Uuid,)> = sqlx::query_as(
                "SELECT talent_id FROM enterprise_bookmarks WHERE enterprise_id = $1 AND talent_id = ANY($2)",
            )
            .bind(eid)
            .bind(&talent_ids)
            .fetch_all(&state.db)
            .await?;
            bookmarked_ids = bookmarks.into_iter().map(|(id,)| id).collect();
        }
    }

    let results: Vec<TalentSearchEntry> = talents
        .iter()
        .map(|t| TalentSearchEntry {
            id: t.id,
            username: t.username.clone(),
            display_name: t.display_name.clone(),
            skill_domain: t.skill_domain.clone(),
            title: t.title.clone(),
            golden_stars: t.golden_stars,
            total_fragments: t.total_fragments,
            streak_current: t.streak_current,
            country: t.country.clone(),
            member_since: t.created_at.to_rfc3339(),
            is_bookmarked: enterprise_id.map(|_| bookmarked_ids.contains(&t.id)),
        })
        .collect();

    Ok(Json(TalentSearchResponse {
        data: results,
        pagination: Pagination {
            page,
            per_page,
            total,
            total_pages: (total as f64 / per_page as f64).ceil() as i64,
        },
        meta: MetaInfo::now(),
    }))
}

/// Lightweight talent card by username. Public, SSR-ready.
#[utoipa::path(
    get,
    path = "/api/talents/{username}/card",
    tag = "enterprise",
    params(("username" = String, Path, description = "Public username")),
    responses(
        (status = 200, description = "Talent card", body = ApiResponse<TalentCardResponse>),
        (status = 404, description = "Talent not found", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn talent_card(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<TalentCardResponse>>, AppError> {
    let talent: Option<TalentResult> = sqlx::query_as(
        "SELECT id, username, display_name, skill_domain, title, golden_stars, total_fragments, streak_current, country, created_at FROM users WHERE username = $1 AND profile_active = TRUE AND is_banned = FALSE",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?;

    let talent = talent.ok_or(AppError::NotFound("Talent not found".to_string()))?;

    // Top 3 skills (source user_skills — skill_fragments droppée en P8.7).
    let top_skills =
        crate::services::SkillsService::list_user_top_skills(&state.db, talent.id, 3).await?;

    // Badge count
    let badge_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_badges WHERE user_id = $1")
            .bind(talent.id)
            .fetch_one(&state.db)
            .await?;

    Ok(Json(ApiResponse::new(TalentCardResponse {
        username: talent.username,
        display_name: talent.display_name,
        skill_domain: talent.skill_domain,
        title: talent.title,
        golden_stars: talent.golden_stars,
        total_fragments: talent.total_fragments,
        streak_current: talent.streak_current,
        country: talent.country,
        member_since: talent.created_at.to_rfc3339(),
        top_skills: top_skills
            .into_iter()
            .map(|(d, s, f)| TalentCardTopSkill {
                domain: d,
                sub_skill: s,
                fragments: f,
            })
            .collect(),
        badge_count,
    })))
}
