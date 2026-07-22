//! Bonjour Skilluv — onboarding flow endpoints.
//!
//! Flow overview (see content strategy doc §9):
//!   1. `POST /api/onboarding/bonjour-skilluv/start` — auto-selects starter template
//!      based on user's primary orientation, calls GitHub API to fork the template
//!      onto the user's account, persists tracking in `onboarding_bonjour_skilluv`.
//!   2. User clones locally, edits HELLO.md, commits, opens PR on their own fork.
//!   3. `GET /api/onboarding/bonjour-skilluv/status` — user polls to see where they
//!      are in the flow (or the frontend uses WebSocket for real-time updates).
//!   4. Webhook handler (not this file) marks `status = completed` when PR is
//!      opened and touches HELLO.md, unlocks the "Bonjour Skilluv" badge, and
//!      creates a hello_wall_entries row.
//!
//! Idempotence: calling `start` twice returns the existing fork instead of
//! creating a duplicate. This matches GitHub's own fork endpoint behavior.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::github as gh;

/// The GitHub organization hosting the `starter-*` template repositories.
const STARTER_ORG: &str = "skilluv-community";

/// Default starter used when we can't map the user's primary orientation to a
/// specific starter. Broad-appeal Node fullstack is the safest fallback.
const DEFAULT_STARTER_SLUG: &str = "starter-fullstack-node";

pub fn onboarding_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/onboarding/bonjour-skilluv/start",
            post(start_bonjour_skilluv),
        )
        .route(
            "/onboarding/bonjour-skilluv/status",
            get(get_bonjour_skilluv_status),
        )
}

fn wrap(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

/// Resolve a starter slug from the user's primary orientation.
///
/// The mapping favors the most likely stack a beginner would use in that
/// orientation. Some orientations don't have a dedicated starter (yet); those
/// fall back to `DEFAULT_STARTER_SLUG`. The user can always override this
/// choice via the `?starter=starter-{slug}` query param on the start endpoint.
///
/// See content strategy annex G for the full list of 15 starters.
fn starter_for_orientation(orientation_slug: &str) -> &'static str {
    match orientation_slug {
        // Fullstack — Rust default (Skilluv signature)
        "dev-fullstack" | "dev-backend" | "systems-programmer" => "starter-fullstack-rust",
        // Frontend
        "dev-frontend" | "web-designer" => "starter-frontend-svelte",
        // Mobile
        "mobile-android" => "starter-mobile-kotlin",
        "mobile-cross" => "starter-mobile-react-native",
        "mobile-ios" => "starter-mobile-react-native", // Swift starter not day-1
        // Game
        "game-programmer" => "starter-game-godot",
        "game-designer" | "game-artist-2d" | "game-artist-3d" | "game-sound-engineer" => {
            "starter-game-godot"
        }
        // Data / AI
        "data-engineer" | "data-analyst" | "ml-engineer" | "prompt-engineer" => {
            "starter-data-python"
        }
        // Embarqué / IoT (new orientation added by migration 0105)
        "dev-embarque-iot" => "starter-iot-esp32",
        // Ops
        "devops-engineer" | "sre" | "cloud-architect" => "starter-devops",
        // Soft skills / misc
        "tech-writer" | "open-source-maintainer" => "starter-frontend-htmx",
        // Everything else → default
        _ => DEFAULT_STARTER_SLUG,
    }
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/onboarding/bonjour-skilluv/start
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct StartQuery {
    /// Optional override for the starter slug. If omitted, we auto-select from
    /// the user's primary orientation.
    #[serde(default)]
    starter: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct OnboardingRow {
    user_id: Uuid,
    starter_slug: String,
    fork_full_name: String,
    fork_html_url: String,
    github_fork_id: i64,
    status: String,
    pr_number: Option<i32>,
    pr_url: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    pr_opened_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn start_bonjour_skilluv(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<StartQuery>,
) -> Result<Json<Value>, AppError> {
    // ── 1. Idempotence: existing row = return it, no fork call
    if let Some(existing) = load_row(&state.db, auth.user_id).await? {
        return Ok(Json(wrap(json!({
            "already_started": true,
            "onboarding": row_to_json(&existing),
        }))));
    }

    // ── 2. Ensure user has connected GitHub
    let access_token = gh::load_token(&state.db, &state.config.jwt_secret, auth.user_id)
        .await?
        .ok_or_else(|| {
            AppError::Validation(
                "GitHub account not connected. Connect via /api/github/oauth first.".into(),
            )
        })?;

    // ── 3. Resolve starter slug
    let starter_slug = if let Some(explicit) = q.starter {
        validate_starter_slug(&explicit)?;
        explicit
    } else {
        // Look up user's primary (first) orientation
        let orientation_slug: Option<String> = sqlx::query_scalar(
            r#"
            SELECT o.slug
            FROM user_orientations uo
            JOIN orientations o ON o.id = uo.orientation_id
            WHERE uo.user_id = $1 AND uo.is_primary = TRUE AND uo.mode = 'active'
            LIMIT 1
            "#,
        )
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await?;

        match orientation_slug {
            Some(slug) => starter_for_orientation(&slug).to_string(),
            None => DEFAULT_STARTER_SLUG.to_string(),
        }
    };

    let source_full_name = format!("{STARTER_ORG}/{starter_slug}");

    // ── 4. Fork the template via GitHub API
    let fork = gh::fork_repo(&access_token, &source_full_name).await?;

    // ── 5. Persist tracking row
    let row: OnboardingRow = sqlx::query_as(
        r#"
        INSERT INTO onboarding_bonjour_skilluv
            (user_id, starter_slug, fork_full_name, fork_html_url, github_fork_id, status)
        VALUES ($1, $2, $3, $4, $5, 'forked')
        RETURNING user_id, starter_slug, fork_full_name, fork_html_url, github_fork_id,
                  status, pr_number, pr_url, started_at, pr_opened_at, completed_at
        "#,
    )
    .bind(auth.user_id)
    .bind(&starter_slug)
    .bind(&fork.full_name)
    .bind(&fork.html_url)
    .bind(fork.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(wrap(json!({
        "already_started": false,
        "onboarding": row_to_json(&row),
        "next_steps": {
            "clone_url": format!("git@github.com:{}.git", fork.full_name),
            "instructions_key": "onboarding.bonjour_skilluv.instructions",
        },
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/onboarding/bonjour-skilluv/status
// ═══════════════════════════════════════════════════════════════════

async fn get_bonjour_skilluv_status(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    match load_row(&state.db, auth.user_id).await? {
        Some(row) => Ok(Json(wrap(json!({
            "started": true,
            "onboarding": row_to_json(&row),
        })))),
        None => Ok(Json(wrap(json!({
            "started": false,
            "onboarding": null,
        })))),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

async fn load_row(db: &sqlx::PgPool, user_id: Uuid) -> Result<Option<OnboardingRow>, AppError> {
    let row: Option<OnboardingRow> = sqlx::query_as(
        r#"
        SELECT user_id, starter_slug, fork_full_name, fork_html_url, github_fork_id,
               status, pr_number, pr_url, started_at, pr_opened_at, completed_at
        FROM onboarding_bonjour_skilluv
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

fn row_to_json(row: &OnboardingRow) -> Value {
    json!({
        "starter_slug": row.starter_slug,
        "fork_full_name": row.fork_full_name,
        "fork_html_url": row.fork_html_url,
        "status": row.status,
        "pr_number": row.pr_number,
        "pr_url": row.pr_url,
        "started_at": row.started_at.to_rfc3339(),
        "pr_opened_at": row.pr_opened_at.map(|d| d.to_rfc3339()),
        "completed_at": row.completed_at.map(|d| d.to_rfc3339()),
    })
}

fn validate_starter_slug(slug: &str) -> Result<(), AppError> {
    if !slug.starts_with("starter-")
        || slug.len() < 10
        || slug.len() > 60
        || !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::Validation(
            "starter must match pattern 'starter-[a-z0-9-]+' and be 10-60 chars".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_maps_to_expected_starter() {
        assert_eq!(
            starter_for_orientation("dev-fullstack"),
            "starter-fullstack-rust"
        );
        assert_eq!(
            starter_for_orientation("dev-frontend"),
            "starter-frontend-svelte"
        );
        assert_eq!(
            starter_for_orientation("mobile-android"),
            "starter-mobile-kotlin"
        );
        assert_eq!(
            starter_for_orientation("dev-embarque-iot"),
            "starter-iot-esp32"
        );
        assert_eq!(
            starter_for_orientation("game-programmer"),
            "starter-game-godot"
        );
        assert_eq!(
            starter_for_orientation("ml-engineer"),
            "starter-data-python"
        );
        assert_eq!(starter_for_orientation("devops-engineer"), "starter-devops");
        assert_eq!(
            starter_for_orientation("tech-writer"),
            "starter-frontend-htmx"
        );
        // Unknown orientation → default
        assert_eq!(
            starter_for_orientation("unknown-slug"),
            DEFAULT_STARTER_SLUG
        );
    }

    #[test]
    fn validate_starter_slug_accepts_valid() {
        assert!(validate_starter_slug("starter-fullstack-rust").is_ok());
        assert!(validate_starter_slug("starter-iot-esp32").is_ok());
        assert!(validate_starter_slug("starter-devops").is_ok());
    }

    #[test]
    fn validate_starter_slug_rejects_invalid() {
        assert!(validate_starter_slug("").is_err());
        assert!(validate_starter_slug("no-prefix").is_err());
        assert!(validate_starter_slug("starter-").is_err()); // too short
        assert!(validate_starter_slug("starter-UPPER").is_err()); // uppercase
        assert!(validate_starter_slug("starter-under_score").is_err()); // underscore
        assert!(validate_starter_slug("starter-slash/hack").is_err()); // slash (attack surface)
        // Length overflow
        let too_long = format!("starter-{}", "x".repeat(60));
        assert!(validate_starter_slug(&too_long).is_err());
    }
}
