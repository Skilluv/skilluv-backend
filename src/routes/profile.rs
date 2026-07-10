use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::models::BadgeWithEarnedAt;
use crate::services::LeaderboardService;

pub fn profile_routes() -> Router<AppState> {
    Router::new().route("/profile/{username}", get(public_profile))
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

#[derive(Debug, sqlx::FromRow)]
struct ProfileUser {
    id: Uuid,
    username: String,
    display_name: String,
    skill_domain: String,
    title: String,
    golden_stars: i32,
    total_fragments: i32,
    streak_current: i32,
    country: Option<String>,
    city: Option<String>,
    bio: Option<String>,
    avatar_url: Option<String>,
    github: Option<String>,
    linkedin: Option<String>,
    website: Option<String>,
    twitter: Option<String>,
    profile_active: bool,
    is_banned: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct ActivityDay {
    activity_date: chrono::NaiveDate,
    challenges_completed: i32,
    fragments_earned: i32,
}

// GET /api/profile/{username} — public profile (no auth, SSR-ready)
async fn public_profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user: ProfileUser = sqlx::query_as(
        "SELECT id, username, display_name, skill_domain, title, golden_stars, total_fragments, streak_current, country, city, bio, avatar_url, github, linkedin, website, twitter, profile_active, is_banned, created_at FROM users WHERE username = $1",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("User not found".to_string()))?;

    if !user.profile_active || user.is_banned {
        return Err(AppError::NotFound("User not found".to_string()));
    }

    let thirty_days_ago = chrono::Utc::now().date_naive() - chrono::Duration::days(30);

    // Load privacy settings
    #[derive(Debug, sqlx::FromRow)]
    struct Privacy {
        show_heatmap: bool,
        show_skill_tree: bool,
        show_badges: bool,
        show_streak: bool,
    }

    let privacy: Privacy = sqlx::query_as(
        r#"
        SELECT COALESCE(show_heatmap, TRUE) as show_heatmap,
               COALESCE(show_skill_tree, TRUE) as show_skill_tree,
               COALESCE(show_badges, TRUE) as show_badges,
               COALESCE(show_streak, TRUE) as show_streak
        FROM user_privacy WHERE user_id = $1
        "#,
    )
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .unwrap_or(Privacy {
        show_heatmap: true,
        show_skill_tree: true,
        show_badges: true,
        show_streak: true,
    });

    // Run parallel queries.
    // P8.6 : les skill_fragments retombent sur user_skills en fallback.
    // La signature du helper retourne AppError alors que les autres futures
    // retournent sqlx::Error — on encapsule manuellement pour try_join!.
    let (fragments_result, challenges_count_result, heatmap_result, badges_result) = tokio::try_join!(
        async {
            crate::services::SkillsService::list_user_skill_fragments_or_backfill(
                &state.db,
                user.id,
                crate::services::SkillFragmentOrder::ByFragmentsDesc,
            )
            .await
            .map_err(|_| sqlx::Error::RowNotFound)
        },

        // Challenges completed count
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM challenge_submissions WHERE user_id = $1 AND status = 'success'"
        )
        .bind(user.id)
        .fetch_one(&state.db),

        // Heatmap last 30 days
        sqlx::query_as::<_, ActivityDay>(
            "SELECT activity_date, challenges_completed, fragments_earned FROM user_activity WHERE user_id = $1 AND activity_date >= $2 ORDER BY activity_date"
        )
        .bind(user.id)
        .bind(thirty_days_ago)
        .fetch_all(&state.db),

        // Badges
        sqlx::query_as::<_, BadgeWithEarnedAt>(
            "SELECT b.slug, b.name, b.description, b.icon, b.category, ub.earned_at FROM badges b JOIN user_badges ub ON b.id = ub.badge_id WHERE ub.user_id = $1 ORDER BY ub.earned_at DESC"
        )
        .bind(user.id)
        .fetch_all(&state.db),
    )?;

    // Build skill tree grouped by domain
    let mut domains: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    for f in &fragments_result {
        domains
            .entry(f.skill_domain.clone())
            .or_default()
            .push(json!({
                "sub_skill": f.sub_skill,
                "fragments": f.fragments,
            }));
    }

    let skill_tree: Vec<serde_json::Value> = domains
        .into_iter()
        .map(|(domain, skills)| {
            let total: i32 = skills
                .iter()
                .filter_map(|s| s["fragments"].as_i64())
                .sum::<i64>() as i32;
            json!({
                "domain": domain,
                "total_fragments": total,
                "top_skills": &skills[..skills.len().min(5)],
            })
        })
        .collect();

    // Get ranks from Redis
    let mut redis = state.redis.clone();
    let global_rank =
        LeaderboardService::get_rank(&mut redis, "global", "alltime", user.id).await?;
    let domain_rank =
        LeaderboardService::get_rank(&mut redis, &user.skill_domain, "alltime", user.id).await?;

    let heatmap_days_active = heatmap_result.len();
    let heatmap_data: Vec<serde_json::Value> = heatmap_result
        .iter()
        .map(|a| {
            json!({
                "date": a.activity_date,
                "challenges": a.challenges_completed,
                "fragments": a.fragments_earned,
            })
        })
        .collect();

    let badges_data: Vec<serde_json::Value> = badges_result
        .iter()
        .map(|b| {
            json!({
                "slug": b.slug,
                "name": b.name,
                "description": b.description,
                "icon": b.icon,
                "category": b.category,
                "earned_at": b.earned_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(build_response(json!({
        "user": {
            "username": user.username,
            "display_name": user.display_name,
            "title": user.title,
            "golden_stars": user.golden_stars,
            "skill_domain": user.skill_domain,
            "country": user.country,
            "city": user.city,
            "bio": user.bio,
            "avatar_url": user.avatar_url,
            "github": user.github,
            "linkedin": user.linkedin,
            "website": user.website,
            "twitter": user.twitter,
            "member_since": user.created_at.to_rfc3339(),
        },
        "stats": {
            "total_fragments": user.total_fragments,
            "streak_current": if privacy.show_streak { Some(user.streak_current) } else { None },
            "challenges_completed": challenges_count_result,
            "global_rank": global_rank,
            "domain_rank": domain_rank,
        },
        "skill_tree": if privacy.show_skill_tree { Some(skill_tree) } else { None },
        "heatmap_summary": if privacy.show_heatmap { Some(json!({
            "days_active": heatmap_days_active,
            "last_30_days": heatmap_data,
        })) } else { None },
        "badges": if privacy.show_badges { Some(badges_data) } else { None },
    }))))
}
