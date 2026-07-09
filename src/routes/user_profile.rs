use axum::extract::{Multipart, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, RateLimiter};

const MAX_AVATAR_SIZE: usize = 2 * 1024 * 1024; // 2MB

pub fn user_profile_routes() -> Router<AppState> {
    Router::new()
        .route("/profile/me", put(update_profile))
        .route("/profile/me/avatar", post(upload_avatar))
        .route("/profile/me/avatar", delete(delete_avatar))
        .route("/profile/me/privacy", get(get_privacy))
        .route("/profile/me/privacy", put(update_privacy))
        .route("/auth/me/display-name", put(update_display_name))
        .route("/auth/me/skill-domain", put(update_skill_domain))
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

// ─── Request types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct UpdateProfileRequest {
    bio: Option<String>,
    github: Option<String>,
    linkedin: Option<String>,
    website: Option<String>,
    twitter: Option<String>,
    country: Option<String>,
    city: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateDisplayNameRequest {
    display_name: String,
}

#[derive(Debug, Deserialize)]
struct UpdateSkillDomainRequest {
    skill_domain: String,
}

#[derive(Debug, Deserialize)]
struct UpdatePrivacyRequest {
    show_email: Option<bool>,
    show_heatmap: Option<bool>,
    show_skill_tree: Option<bool>,
    show_badges: Option<bool>,
    show_streak: Option<bool>,
    allow_interest_requests: Option<bool>,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct PrivacySettings {
    show_email: bool,
    show_heatmap: bool,
    show_skill_tree: bool,
    show_badges: bool,
    show_streak: bool,
    allow_interest_requests: bool,
    updated_at: chrono::DateTime<chrono::Utc>,
}

// ─── Routes ─────────────────────────────────────────────────────

// PUT /api/profile/me — update bio, social links, country
async fn update_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    RateLimiter::check(
        &mut state.redis.clone(),
        "profile_update",
        &auth.user_id.to_string(),
        30,
        60,
    )
    .await?;

    // Validate lengths
    if let Some(ref bio) = body.bio {
        if bio.len() > 1000 {
            return Err(AppError::Validation(
                "Bio must be at most 1000 characters".to_string(),
            ));
        }
    }
    if let Some(ref gh) = body.github {
        if gh.len() > 100 {
            return Err(AppError::Validation(
                "GitHub username must be at most 100 characters".to_string(),
            ));
        }
    }
    if let Some(ref li) = body.linkedin {
        if li.len() > 200 {
            return Err(AppError::Validation(
                "LinkedIn URL must be at most 200 characters".to_string(),
            ));
        }
    }
    if let Some(ref ws) = body.website {
        if ws.len() > 500 {
            return Err(AppError::Validation(
                "Website URL must be at most 500 characters".to_string(),
            ));
        }
    }
    if let Some(ref tw) = body.twitter {
        if tw.len() > 100 {
            return Err(AppError::Validation(
                "Twitter handle must be at most 100 characters".to_string(),
            ));
        }
    }
    if let Some(ref country) = body.country {
        if country.len() > 3 {
            return Err(AppError::Validation(
                "Country must be an ISO-3 code (max 3 characters)".to_string(),
            ));
        }
    }
    if let Some(ref city) = body.city {
        if city.len() > 100 {
            return Err(AppError::Validation(
                "City must be at most 100 characters".to_string(),
            ));
        }
    }

    let user: crate::models::User = sqlx::query_as(
        r#"
        UPDATE users SET
            bio = COALESCE($1, bio),
            github = COALESCE($2, github),
            linkedin = COALESCE($3, linkedin),
            website = COALESCE($4, website),
            twitter = COALESCE($5, twitter),
            country = COALESCE($6, country),
            city = COALESCE($7, city),
            updated_at = NOW()
        WHERE id = $8
        RETURNING *
        "#,
    )
    .bind(&body.bio)
    .bind(&body.github)
    .bind(&body.linkedin)
    .bind(&body.website)
    .bind(&body.twitter)
    .bind(&body.country)
    .bind(&body.city)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    let private: crate::models::UserPrivate = user.into();

    Ok(Json(build_response(json!({ "user": private }))))
}

// POST /api/profile/me/avatar — upload avatar (multipart)
async fn upload_avatar(
    State(state): State<AppState>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut file_data: Option<(Vec<u8>, String)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("Invalid multipart data: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name != "avatar" {
            continue;
        }

        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        if !["image/jpeg", "image/png", "image/webp"].contains(&content_type.as_str()) {
            return Err(AppError::Validation(
                "Avatar must be JPEG, PNG, or WebP".to_string(),
            ));
        }

        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::Validation(format!("Failed to read file: {e}")))?;

        if data.len() > MAX_AVATAR_SIZE {
            return Err(AppError::Validation(
                "Avatar must be at most 2MB".to_string(),
            ));
        }

        file_data = Some((data.to_vec(), content_type));
        break;
    }

    let (data, content_type) = file_data.ok_or(AppError::Validation(
        "No 'avatar' field found in upload".to_string(),
    ))?;

    // Delete old avatar
    state.storage.delete_avatar(auth.user_id).await?;

    // Upload new avatar
    let key = state
        .storage
        .upload_avatar(auth.user_id, &data, &content_type)
        .await?;

    let avatar_url = state.storage.avatar_url(&key);

    // Update user record
    sqlx::query("UPDATE users SET avatar_url = $1, updated_at = NOW() WHERE id = $2")
        .bind(&avatar_url)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "avatar_url": avatar_url,
        "message": "Avatar uploaded successfully"
    }))))
}

// DELETE /api/profile/me/avatar
async fn delete_avatar(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    state.storage.delete_avatar(auth.user_id).await?;

    sqlx::query("UPDATE users SET avatar_url = NULL, updated_at = NOW() WHERE id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "message": "Avatar deleted"
    }))))
}

// GET /api/profile/me/privacy
async fn get_privacy(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    // Upsert default privacy settings
    let privacy: PrivacySettings = sqlx::query_as(
        r#"
        INSERT INTO user_privacy (user_id)
        VALUES ($1)
        ON CONFLICT (user_id) DO UPDATE SET user_id = user_privacy.user_id
        RETURNING show_email, show_heatmap, show_skill_tree, show_badges, show_streak, allow_interest_requests, updated_at
        "#,
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "privacy": privacy }))))
}

// PUT /api/profile/me/privacy
async fn update_privacy(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdatePrivacyRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let privacy: PrivacySettings = sqlx::query_as(
        r#"
        INSERT INTO user_privacy (user_id, show_email, show_heatmap, show_skill_tree, show_badges, show_streak, allow_interest_requests)
        VALUES ($1, COALESCE($2, FALSE), COALESCE($3, TRUE), COALESCE($4, TRUE), COALESCE($5, TRUE), COALESCE($6, TRUE), COALESCE($7, TRUE))
        ON CONFLICT (user_id) DO UPDATE SET
            show_email = COALESCE($2, user_privacy.show_email),
            show_heatmap = COALESCE($3, user_privacy.show_heatmap),
            show_skill_tree = COALESCE($4, user_privacy.show_skill_tree),
            show_badges = COALESCE($5, user_privacy.show_badges),
            show_streak = COALESCE($6, user_privacy.show_streak),
            allow_interest_requests = COALESCE($7, user_privacy.allow_interest_requests),
            updated_at = NOW()
        RETURNING show_email, show_heatmap, show_skill_tree, show_badges, show_streak, allow_interest_requests, updated_at
        "#,
    )
    .bind(auth.user_id)
    .bind(body.show_email)
    .bind(body.show_heatmap)
    .bind(body.show_skill_tree)
    .bind(body.show_badges)
    .bind(body.show_streak)
    .bind(body.allow_interest_requests)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "privacy": privacy }))))
}

// PUT /api/auth/me/display-name
async fn update_display_name(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdateDisplayNameRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let trimmed = body.display_name.trim();
    if trimmed.is_empty() || trimmed.len() > 100 {
        return Err(AppError::Validation(
            "Display name must be between 1 and 100 characters".to_string(),
        ));
    }

    sqlx::query("UPDATE users SET display_name = $1, updated_at = NOW() WHERE id = $2")
        .bind(trimmed)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "display_name": trimmed,
        "message": "Display name updated"
    }))))
}

// PUT /api/auth/me/skill-domain
async fn update_skill_domain(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdateSkillDomainRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    match body.skill_domain.as_str() {
        "code" | "design" | "game" | "security" => {}
        _ => {
            return Err(AppError::Validation(
                "skill_domain must be one of: code, design, game, security".to_string(),
            ));
        }
    }

    sqlx::query("UPDATE users SET skill_domain = $1, updated_at = NOW() WHERE id = $2")
        .bind(&body.skill_domain)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "skill_domain": body.skill_domain,
        "message": "Skill domain updated"
    }))))
}
