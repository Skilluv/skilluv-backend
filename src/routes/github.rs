//! GitHub OAuth + sync + CV endpoints — Phase 2 Sprint 5.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::routes::analytics_consent;
use crate::services::analytics::{events, props};
use crate::services::{github, projects};

pub fn github_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/github/start", get(start))
        .route("/auth/github/callback", get(callback))
        .route("/auth/github/disconnect", post(disconnect))
        .route("/auth/github/sync", post(sync_now))
        .route("/u/{username}/repos", get(public_repos))
        .route("/u/{username}/cv", get(cv_html))
        // Admin sync
        .route("/admin/github/sync/{user_id}", post(admin_sync))
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

fn github_oauth_env() -> Result<(String, String, String), AppError> {
    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| AppError::Internal("GITHUB_CLIENT_ID not set".into()))?;
    let client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .map_err(|_| AppError::Internal("GITHUB_CLIENT_SECRET not set".into()))?;
    let redirect = std::env::var("GITHUB_REDIRECT_URI")
        .map_err(|_| AppError::Internal("GITHUB_REDIRECT_URI not set".into()))?;
    Ok((client_id, client_secret, redirect))
}

async fn start(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Redirect, AppError> {
    let (client_id, _, redirect_uri) = github_oauth_env()?;
    // State token bound to the user, 15-min TTL in Redis.
    let state_token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let key = format!("gh_oauth_state:{state_token}");
    let mut redis = state.redis.clone();
    let () = redis
        .set_ex(&key, auth.user_id.to_string(), 15 * 60)
        .await?;

    let url = github::build_authorize_url(&client_id, &redirect_uri, &state_token);
    Ok(Redirect::to(&url))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Result<Json<Value>, AppError> {
    let (client_id, client_secret, redirect_uri) = github_oauth_env()?;
    let mut redis = state.redis.clone();
    let key = format!("gh_oauth_state:{}", q.state);
    let user_id_str: Option<String> = redis.get(&key).await?;
    let user_id_str = user_id_str.ok_or(AppError::Unauthorized)?;
    let _: () = redis.del(&key).await?;
    let user_id = Uuid::parse_str(&user_id_str).map_err(|_| AppError::Unauthorized)?;

    let (token, scopes) =
        github::exchange_code(&client_id, &client_secret, &redirect_uri, &q.code).await?;
    let gh_user = github::fetch_user(&token).await?;

    let (encrypted, nonce) =
        github::encrypt_token(&state.config.jwt_secret, &token)?;
    github::upsert_connection(
        &state.db,
        user_id,
        gh_user.id,
        &gh_user.login,
        scopes.as_deref(),
        &encrypted,
        &nonce,
    )
    .await?;

    // Mirror GitHub username onto the user's profile.github field if empty
    let _ = sqlx::query(
        "UPDATE users SET github = COALESCE(NULLIF(github, ''), $1) WHERE id = $2",
    )
    .bind(&gh_user.login)
    .bind(user_id)
    .execute(&state.db)
    .await;

    // Kick off an initial repo sync (best-effort)
    let db = state.db.clone();
    let jwt = state.config.jwt_secret.clone();
    tokio::spawn(async move {
        if let Err(err) = github::sync_repos_for(&db, &jwt, user_id).await {
            tracing::warn!(%user_id, error = %err, "initial github sync failed");
        }
    });

    if analytics_consent(&headers) {
        state.analytics.track(
            user_id,
            events::GITHUB_CONNECTED,
            props(&[
                ("github_login", json!(gh_user.login)),
                ("github_user_id", json!(gh_user.id)),
            ]),
        );
    }
    metrics::counter!("skilluv_github_connections_total").increment(1);

    Ok(Json(build_response(json!({
        "connected": true,
        "github_login": gh_user.login,
    }))))
}

async fn disconnect(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    github::disconnect(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "disconnected": true }))))
}

async fn sync_now(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let report =
        github::sync_repos_for(&state.db, &state.config.jwt_secret, auth.user_id).await?;
    Ok(Json(build_response(json!({ "sync": report }))))
}

async fn admin_sync(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let report = github::sync_repos_for(&state.db, &state.config.jwt_secret, user_id).await?;
    Ok(Json(build_response(json!({ "sync": report }))))
}

#[derive(Deserialize)]
struct ReposQuery {
    limit: Option<i64>,
}

async fn public_repos(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Query(q): Query<ReposQuery>,
) -> Result<Json<Value>, AppError> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE username = $1 AND profile_active = TRUE AND is_banned = FALSE")
            .bind(&username)
            .fetch_optional(&state.db)
            .await?;
    let (user_id,) = row.ok_or(AppError::NotFound("user not found".into()))?;
    let repos = github::top_repos_for_user(&state.db, user_id, q.limit.unwrap_or(12)).await?;
    Ok(Json(build_response(json!({ "repos": repos }))))
}

async fn cv_html(
    State(state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
) -> Result<(StatusCode, Html<String>), AppError> {
    // Resolve user
    let user_row: Option<(Uuid, String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, i32, String, i32, i32, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as(
            r#"
            SELECT id, username, display_name, skill_domain, country, city, bio, avatar_url, total_fragments, title, golden_stars, streak_current, created_at
            FROM users
            WHERE username = $1 AND profile_active = TRUE AND is_banned = FALSE
            "#,
        )
        .bind(&username)
        .fetch_optional(&state.db)
        .await?;
    let Some((
        user_id,
        username_db,
        display_name,
        skill_domain,
        country,
        city,
        bio,
        avatar_url,
        total_fragments,
        title,
        golden_stars,
        streak_current,
        created_at,
    )) = user_row
    else {
        return Err(AppError::NotFound("user not found".into()));
    };

    // Top 5 successful submissions
    let top_subs: Vec<(String, i32, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT c.title, cs.fragments_earned, cs.evaluated_at
        FROM challenge_submissions cs JOIN challenge_templates c ON c.id = cs.challenge_id
        WHERE cs.user_id = $1 AND cs.status = 'success' AND cs.evaluated_at IS NOT NULL
        ORDER BY cs.fragments_earned DESC, cs.evaluated_at DESC LIMIT 5
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    // Top 3 skills (source user_skills — skill_fragments droppée en P8.7).
    let top_skills =
        crate::services::SkillsService::list_user_top_skills(&state.db, user_id, 3).await?;

    // Badges (slug + name)
    let badges: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT b.slug, b.name FROM badges b
        JOIN user_badges ub ON ub.badge_id = b.id WHERE ub.user_id = $1 ORDER BY ub.earned_at DESC LIMIT 12
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    // Projects (owned by this user)
    let user_projects =
        projects::list_for_owner(&state.db, "user", user_id).await.unwrap_or_default();

    // Top 5 GitHub repos (if connected)
    let repos = github::top_repos_for_user(&state.db, user_id, 5).await.unwrap_or_default();

    // Track analytics view
    if analytics_consent(&headers) {
        state.analytics.track(
            user_id,
            events::CV_VIEWED,
            props(&[("username", json!(username_db))]),
        );
    }
    metrics::counter!("skilluv_cv_views_total").increment(1);

    let html = render_cv_html(CvContext {
        username: &username_db,
        display_name: &display_name,
        skill_domain: &skill_domain,
        country: country.as_deref(),
        city: city.as_deref(),
        bio: bio.as_deref(),
        avatar_url: avatar_url.as_deref(),
        total_fragments,
        title: &title,
        golden_stars,
        streak_current,
        member_since: created_at.format("%Y-%m-%d").to_string(),
        top_subs: &top_subs,
        top_skills: &top_skills,
        badges: &badges,
        projects: &user_projects,
        repos: &repos,
    });

    Ok((StatusCode::OK, Html(html)))
}

struct CvContext<'a> {
    username: &'a str,
    display_name: &'a str,
    skill_domain: &'a str,
    country: Option<&'a str>,
    city: Option<&'a str>,
    bio: Option<&'a str>,
    avatar_url: Option<&'a str>,
    total_fragments: i32,
    title: &'a str,
    golden_stars: i32,
    streak_current: i32,
    member_since: String,
    top_subs: &'a [(String, i32, chrono::DateTime<chrono::Utc>)],
    top_skills: &'a [(String, String, i32)],
    badges: &'a [(String, String)],
    projects: &'a [projects::Project],
    repos: &'a [github::RepoSummary],
}

fn render_cv_html(c: CvContext) -> String {
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    let location = match (c.city, c.country) {
        (Some(city), Some(co)) => format!("{} · {}", esc(city), esc(co)),
        (None, Some(co)) => esc(co),
        (Some(city), None) => esc(city),
        _ => String::new(),
    };
    let avatar = c
        .avatar_url
        .map(|u| format!("<img src=\"{}\" alt=\"avatar\" class=\"avatar\">", esc(u)))
        .unwrap_or_default();
    let bio = c.bio.map(esc).unwrap_or_default();
    let subs: String = c
        .top_subs
        .iter()
        .map(|(t, f, _)| format!("<li><strong>{}</strong> <span class=\"frags\">+{f} frag.</span></li>", esc(t)))
        .collect();
    let skills: String = c
        .top_skills
        .iter()
        .map(|(d, s, f)| format!("<li>{} · {} <span class=\"frags\">{f} frag.</span></li>", esc(d), esc(s)))
        .collect();
    let badges: String = c
        .badges
        .iter()
        .map(|(_, name)| format!("<span class=\"badge\">{}</span>", esc(name)))
        .collect();
    let projects: String = c
        .projects
        .iter()
        .map(|p| {
            let stack = p.tech_stack.iter().map(|s| esc(s)).collect::<Vec<_>>().join(" · ");
            let repo_link = p
                .repo_url
                .as_deref()
                .map(|u| format!("<a href=\"{}\">repo</a>", esc(u)))
                .unwrap_or_default();
            let demo_link = p
                .demo_url
                .as_deref()
                .map(|u| format!("<a href=\"{}\">demo</a>", esc(u)))
                .unwrap_or_default();
            format!(
                "<div class=\"proj\"><h4>{}</h4><p>{}</p><p class=\"meta\">{stack} · {repo_link} {demo_link}</p></div>",
                esc(&p.name),
                esc(p.description.as_deref().unwrap_or(""))
            )
        })
        .collect();
    let repos: String = c
        .repos
        .iter()
        .map(|r| {
            format!(
                "<div class=\"proj\"><h4><a href=\"{url}\">{name}</a> <span class=\"meta\">★ {stars}</span></h4><p>{desc}</p><p class=\"meta\">{lang}</p></div>",
                url = esc(&r.html_url),
                name = esc(&r.full_name),
                stars = r.stargazers_count,
                desc = esc(r.description.as_deref().unwrap_or("")),
                lang = esc(r.language.as_deref().unwrap_or("")),
            )
        })
        .collect();

    format!(
        r#"<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<title>CV — {display} ({username})</title>
<meta name="robots" content="noindex">
<style>
  :root {{ --accent: #6c5ce7; --text: #1a1a2e; --muted: #666; }}
  body {{ font-family: -apple-system, system-ui, Inter, Arial, sans-serif; color: var(--text); max-width: 820px; margin: 30px auto; padding: 0 24px; line-height: 1.45; }}
  header {{ display: flex; gap: 20px; align-items: center; border-bottom: 1px solid #eee; padding-bottom: 16px; }}
  .avatar {{ width: 96px; height: 96px; border-radius: 50%; object-fit: cover; }}
  h1 {{ margin: 0; }}
  h2 {{ color: var(--accent); margin-top: 28px; font-size: 17px; text-transform: uppercase; letter-spacing: 1px; }}
  .meta, .frags {{ color: var(--muted); font-size: 13px; }}
  ul {{ padding-left: 18px; }}
  .badge {{ display: inline-block; background: #f0eeff; color: var(--accent); padding: 4px 10px; border-radius: 14px; margin: 3px 4px 3px 0; font-size: 12px; }}
  .proj {{ padding: 10px 0; border-top: 1px dashed #eee; }}
  .proj h4 {{ margin: 0 0 4px; }}
  .grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }}
  footer {{ margin-top: 40px; font-size: 11px; color: var(--muted); border-top: 1px solid #eee; padding-top: 10px; }}
  @media print {{ a {{ color: inherit; text-decoration: none; }} }}
</style>
</head>
<body>
<header>
  {avatar}
  <div>
    <h1>{display}</h1>
    <p class="meta">@{username} · {domain} · {location}</p>
    <p class="meta">{title} · {frags} fragments · 🔥 streak {streak} · ⭐ {gstars}</p>
  </div>
</header>

<section>
  <h2>À propos</h2>
  <p>{bio}</p>
</section>

<div class="grid">
  <section>
    <h2>Top challenges</h2>
    <ul>{subs}</ul>
  </section>
  <section>
    <h2>Top compétences</h2>
    <ul>{skills}</ul>
  </section>
</div>

<section>
  <h2>Badges</h2>
  <div>{badges}</div>
</section>

<section>
  <h2>Projets Skilluv</h2>
  {projects_or_empty}
</section>

<section>
  <h2>Repos GitHub (top 5)</h2>
  {repos_or_empty}
</section>

<footer>
  Membre depuis {since} · CV généré par Skilluv · skilluv.com/u/{username}/cv · Imprime cette page (Ctrl/Cmd+P) pour PDF.
</footer>
</body>
</html>"#,
        display = esc(c.display_name),
        username = esc(c.username),
        domain = esc(c.skill_domain),
        title = esc(c.title),
        frags = c.total_fragments,
        streak = c.streak_current,
        gstars = c.golden_stars,
        since = c.member_since,
        location = location,
        avatar = avatar,
        bio = bio,
        subs = if subs.is_empty() {
            "<li class=\"meta\">Aucun challenge complété pour le moment.</li>".to_string()
        } else {
            subs
        },
        skills = if skills.is_empty() {
            "<li class=\"meta\">Aucune compétence enregistrée.</li>".to_string()
        } else {
            skills
        },
        badges = if badges.is_empty() {
            "<span class=\"meta\">Aucun badge encore.</span>".to_string()
        } else {
            badges
        },
        projects_or_empty = if projects.is_empty() {
            "<p class=\"meta\">Aucun projet listé.</p>".to_string()
        } else {
            projects
        },
        repos_or_empty = if repos.is_empty() {
            "<p class=\"meta\">GitHub non connecté ou aucun repo public.</p>".to_string()
        } else {
            repos
        },
    )
}
