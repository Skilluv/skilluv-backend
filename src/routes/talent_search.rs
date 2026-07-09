use axum::extract::{Path, Query, State};
use axum::http::request::Parts;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::AuthService;

pub fn talent_search_routes() -> Router<AppState> {
    Router::new()
        .route("/talents/search", get(search_talents))
        .route("/talents/{username}/card", get(talent_card))
}

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: Option<String>,
    skill_domain: Option<String>,
    title: Option<String>,
    country: Option<String>,
    min_fragments: Option<i32>,
    sort_by: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
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
    Some(AuthUser {
        user_id,
        role: claims.role,
        login_method: claims
            .login_method
            .unwrap_or_else(|| "password".to_string()),
    })
}

// GET /api/talents/search — no auth required (SSR-ready), optional auth for enterprise features
async fn search_talents(
    State(state): State<AppState>,
    parts: Parts,
    Query(query): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
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
    let mut db_query = sqlx::query_as::<_, TalentResult>(&sql);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);

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

    let results: Vec<serde_json::Value> = talents
        .iter()
        .map(|t| {
            let mut entry = json!({
                "id": t.id,
                "username": t.username,
                "display_name": t.display_name,
                "skill_domain": t.skill_domain,
                "title": t.title,
                "golden_stars": t.golden_stars,
                "total_fragments": t.total_fragments,
                "streak_current": t.streak_current,
                "country": t.country,
                "member_since": t.created_at.to_rfc3339(),
            });
            if enterprise_id.is_some() {
                entry["is_bookmarked"] = json!(bookmarked_ids.contains(&t.id));
            }
            entry
        })
        .collect();

    Ok(Json(json!({
        "data": results,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": (total as f64 / per_page as f64).ceil() as i64,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

// GET /api/talents/{username}/card — lightweight talent card (no auth, SSR-ready)
async fn talent_card(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let talent: Option<TalentResult> = sqlx::query_as(
        "SELECT id, username, display_name, skill_domain, title, golden_stars, total_fragments, streak_current, country, created_at FROM users WHERE username = $1 AND profile_active = TRUE AND is_banned = FALSE",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?;

    let talent = talent.ok_or(AppError::NotFound("Talent not found".to_string()))?;

    // Get top 3 skills
    let top_skills: Vec<(String, String, i32)> = sqlx::query_as(
        "SELECT skill_domain, sub_skill, fragments FROM skill_fragments WHERE user_id = $1 ORDER BY fragments DESC LIMIT 3",
    )
    .bind(talent.id)
    .fetch_all(&state.db)
    .await?;

    // Badge count
    let badge_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_badges WHERE user_id = $1")
            .bind(talent.id)
            .fetch_one(&state.db)
            .await?;

    Ok(Json(build_response(json!({
        "username": talent.username,
        "display_name": talent.display_name,
        "skill_domain": talent.skill_domain,
        "title": talent.title,
        "golden_stars": talent.golden_stars,
        "total_fragments": talent.total_fragments,
        "streak_current": talent.streak_current,
        "country": talent.country,
        "member_since": talent.created_at.to_rfc3339(),
        "top_skills": top_skills.iter().map(|(d, s, f)| json!({"domain": d, "sub_skill": s, "fragments": f})).collect::<Vec<_>>(),
        "badge_count": badge_count,
    }))))
}
