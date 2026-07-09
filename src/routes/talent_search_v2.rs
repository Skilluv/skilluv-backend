//! Advanced talent search — Phase 4.7.
//!
//! Adds 8 new filter dimensions on top of the v1 endpoint: tags, badges, min_streak,
//! looking_for, available_only, language_spoken, min_github_repos, has_projects.
//! Results are enriched with top_skills, badge_count, project_count.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::OptionalAuth;

pub fn talent_search_v2_routes() -> Router<AppState> {
    Router::new().route("/talents/search/v2", get(search_v2))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: Option<String>,
    skill_domain: Option<String>,
    title: Option<String>,
    country: Option<String>,
    /// ISO2 (Phase 3.3). Falls back to legacy `country` (ISO3) if not provided.
    country_iso2: Option<String>,
    min_fragments: Option<i32>,
    min_streak: Option<i32>,
    tag: Option<String>,               // tag slug — multiple joins if repeated
    badge: Option<String>,             // badge slug
    looking_for: Option<String>,       // cdi | cdd | freelance | internship | contract
    available_only: Option<bool>,
    language_spoken: Option<String>,   // 2-letter ISO code (min B2)
    has_projects: Option<bool>,
    min_github_repos: Option<i32>,
    sort_by: Option<String>,           // fragments | recent | most_active_recently | top_in_domain
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn search_v2(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    tenant: crate::middleware::TenantContext,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Value>, AppError> {
    let per_page = q.per_page.unwrap_or(20).clamp(1, 50);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    // Build filters as (SQL fragment, bind index) pairs.
    // We collect binds in an enum so we can rebind them in order.
    let mut wheres: Vec<String> = Vec::new();
    let mut binds: Vec<Bind> = Vec::new();

    macro_rules! push {
        ($expr:expr, $val:expr) => {
            let idx = binds.len() + 1;
            wheres.push($expr.replace("{i}", &idx.to_string()));
            binds.push($val);
        };
    }

    if let Some(ref s) = q.skill_domain {
        push!("u.skill_domain = ${i}", Bind::Str(s.clone()));
    }
    if let Some(ref s) = q.title {
        push!("u.title = ${i}", Bind::Str(s.clone()));
    }
    if let Some(ref s) = q.country_iso2 {
        push!("u.country_iso2 = UPPER(${i})", Bind::Str(s.clone()));
    } else if let Some(ref s) = q.country {
        push!("u.country = ${i}", Bind::Str(s.clone()));
    }
    if let Some(v) = q.min_fragments {
        push!("u.total_fragments >= ${i}", Bind::I32(v));
    }
    if let Some(v) = q.min_streak {
        push!("u.streak_current >= ${i}", Bind::I32(v));
    }
    if let Some(ref s) = q.looking_for {
        push!("u.looking_for = ${i}", Bind::Str(s.clone()));
    }
    if let Some(true) = q.available_only {
        wheres.push("u.available_for_hire = TRUE".into());
    }
    if let Some(true) = q.has_projects {
        wheres.push(
            "EXISTS (SELECT 1 FROM projects p WHERE p.owner_type = 'user' AND p.owner_id = u.id AND p.archived_at IS NULL)".into(),
        );
    }
    if let Some(v) = q.min_github_repos {
        push!(
            "(SELECT COUNT(*) FROM github_repos gr WHERE gr.user_id = u.id AND gr.archived = FALSE AND gr.fork = FALSE) >= ${i}",
            Bind::I32(v)
        );
    }
    if let Some(ref s) = q.language_spoken {
        push!(
            "EXISTS (SELECT 1 FROM user_languages l WHERE l.user_id = u.id AND l.language = LOWER(${i}) AND l.proficiency IN ('B2', 'C1', 'C2', 'native'))",
            Bind::Str(s.clone())
        );
    }
    if let Some(ref s) = q.tag {
        push!(
            "EXISTS (SELECT 1 FROM tag_map m JOIN tags t ON t.id = m.tag_id WHERE m.target_type = 'user' AND m.target_id = u.id AND t.slug = ${i})",
            Bind::Str(s.clone())
        );
    }
    if let Some(ref s) = q.badge {
        push!(
            "EXISTS (SELECT 1 FROM user_badges ub JOIN badges b ON b.id = ub.badge_id WHERE ub.user_id = u.id AND b.slug = ${i})",
            Bind::Str(s.clone())
        );
    }
    if let Some(ref term) = q.q {
        push!(
            "u.search_vector @@ to_tsquery('simple', ${i})",
            Bind::Str(term.split_whitespace().collect::<Vec<_>>().join(" & "))
        );
    }

    // Phase 5.9 : isolation tenant. Sur un sous-tenant (bootcamp), n'exposer
    // que les talents rattachés (primary tenant OU membership actif). Le SQL
    // référence deux fois le même placeholder — c'est légal côté Postgres.
    if !crate::routes::is_root_tenant(tenant.tenant_id) {
        push!(
            "(u.primary_tenant_id = ${i} OR EXISTS (SELECT 1 FROM tenant_memberships tm WHERE tm.user_id = u.id AND tm.tenant_id = ${i}))",
            Bind::Uuid(tenant.tenant_id)
        );
    }

    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!(" AND {}", wheres.join(" AND "))
    };
    let order_sql = match q.sort_by.as_deref() {
        Some("recent") => "u.updated_at DESC",
        Some("most_active_recently") =>
            "(SELECT MAX(evaluated_at) FROM challenge_submissions cs WHERE cs.user_id = u.id) DESC NULLS LAST",
        Some("top_in_domain") => "u.total_fragments DESC, u.golden_stars DESC",
        _ => "u.total_fragments DESC",
    };
    let sql = format!(
        r#"
        SELECT u.id, u.username, u.display_name, u.skill_domain, u.title, u.golden_stars,
               u.total_fragments, u.streak_current, u.country, u.country_iso2,
               u.available_for_hire, u.looking_for, u.updated_at,
               (SELECT COUNT(*) FROM user_badges ub WHERE ub.user_id = u.id)::BIGINT AS badge_count,
               (SELECT COUNT(*) FROM projects p WHERE p.owner_type = 'user' AND p.owner_id = u.id AND p.archived_at IS NULL)::BIGINT AS project_count,
               (SELECT MAX(evaluated_at) FROM challenge_submissions cs WHERE cs.user_id = u.id) AS last_activity_at
        FROM users u
        WHERE u.role = 'user' AND u.profile_active = TRUE AND u.is_banned = FALSE
        {where_sql}
        ORDER BY {order_sql}
        LIMIT {per_page} OFFSET {offset}
        "#
    );
    let count_sql = format!(
        "SELECT COUNT(*) FROM users u WHERE u.role = 'user' AND u.profile_active = TRUE AND u.is_banned = FALSE{where_sql}",
    );

    let mut db_query = sqlx::query(&sql);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    for b in &binds {
        db_query = b.apply(db_query);
        count_query = b.apply_scalar(count_query);
    }

    let rows = db_query.fetch_all(&state.db).await?;
    let total: i64 = count_query.fetch_one(&state.db).await?;

    use sqlx::Row;
    let mut talents: Vec<Value> = Vec::with_capacity(rows.len());
    for r in &rows {
        let uid: Uuid = r.get("id");
        // Top 3 skills (small extra query — fine at 20 rows/page).
        let top_skills: Vec<(String, String, i32)> = sqlx::query_as(
            r#"
            SELECT skill_domain, sub_skill, fragments FROM skill_fragments
            WHERE user_id = $1 ORDER BY fragments DESC LIMIT 3
            "#,
        )
        .bind(uid)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        talents.push(json!({
            "id": uid,
            "username": r.get::<String, _>("username"),
            "display_name": r.get::<String, _>("display_name"),
            "skill_domain": r.get::<String, _>("skill_domain"),
            "title": r.get::<String, _>("title"),
            "golden_stars": r.get::<i32, _>("golden_stars"),
            "total_fragments": r.get::<i32, _>("total_fragments"),
            "streak_current": r.get::<i32, _>("streak_current"),
            "country": r.get::<Option<String>, _>("country"),
            "country_iso2": r.get::<Option<String>, _>("country_iso2"),
            "available_for_hire": r.get::<bool, _>("available_for_hire"),
            "looking_for": r.get::<Option<String>, _>("looking_for"),
            "badge_count": r.get::<i64, _>("badge_count"),
            "project_count": r.get::<i64, _>("project_count"),
            "last_activity_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_activity_at"),
            "top_skills": top_skills.iter().map(|(d, s, f)| json!({"domain": d, "sub_skill": s, "fragments": f})).collect::<Vec<_>>(),
        }));
    }

    // Enterprise bookmarks lookup (identical to v1)
    if let Some(ref a) = auth {
        if let Ok(Some((eid,))) = sqlx::query_as::<_, (Uuid,)>(
            "SELECT enterprise_id FROM enterprise_members WHERE user_id = $1 AND status = 'active' LIMIT 1",
        )
        .bind(a.user_id)
        .fetch_optional(&state.db)
        .await
        {
            let ids: Vec<Uuid> = talents
                .iter()
                .filter_map(|t| t.get("id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()))
                .collect();
            if !ids.is_empty() {
                let bookmarks: Vec<(Uuid,)> = sqlx::query_as(
                    "SELECT talent_id FROM enterprise_bookmarks WHERE enterprise_id = $1 AND talent_id = ANY($2)",
                )
                .bind(eid)
                .bind(&ids)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default();
                let set: std::collections::HashSet<Uuid> = bookmarks.into_iter().map(|(id,)| id).collect();
                for t in talents.iter_mut() {
                    if let Some(id_str) = t.get("id").and_then(|v| v.as_str()) {
                        if let Ok(uid) = Uuid::parse_str(id_str) {
                            t["is_bookmarked"] = json!(set.contains(&uid));
                        }
                    }
                }
            }
        }
    }

    Ok(Json(json!({
        "data": talents,
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

// ─── Dynamic bind helper ────────────────────────────────────────

enum Bind {
    Str(String),
    I32(i32),
    Uuid(Uuid),
}

impl Bind {
    fn apply<'a>(
        &'a self,
        q: sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments> {
        match self {
            Bind::Str(s) => q.bind(s),
            Bind::I32(v) => q.bind(v),
            Bind::Uuid(u) => q.bind(u),
        }
    }
    fn apply_scalar<'a>(
        &'a self,
        q: sqlx::query::QueryScalar<'a, sqlx::Postgres, i64, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::QueryScalar<'a, sqlx::Postgres, i64, sqlx::postgres::PgArguments> {
        match self {
            Bind::Str(s) => q.bind(s),
            Bind::I32(v) => q.bind(v),
            Bind::Uuid(u) => q.bind(u),
        }
    }
}
