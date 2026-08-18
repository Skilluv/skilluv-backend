//! SKI-47 (Post-MVP T3-04) — skill tree endpoints.
//!
//! Endpoints:
//!   GET /api/users/{id}/skill-tree                       (public if profile is)
//!   PUT /api/admin/skills/{id}/prerequisites             (admin)
//!
//! The read endpoint returns the whole tree with per-node status, which is
//! what a graph rendering needs — it cannot lay out a partial graph.
//! Payload size is bounded by the skill catalog (a few hundred nodes), not
//! by user data, so there is no pagination: paginating a tree would hand
//! the client an unrenderable fragment.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::admin_gate::AdminGate;
use crate::middleware::{AuthUser, OptionalAuth};
use crate::services::skill_tree;

pub fn skill_tree_routes() -> Router<AppState> {
    Router::new().route("/users/{user_id}/skill-tree", get(user_skill_tree))
}

pub fn admin_skill_tree_routes() -> Router<AppState> {
    Router::new().route("/admin/skills/{id}/prerequisites", put(set_prerequisites))
}

fn wrap(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize)]
pub struct SkillTreeQuery {
    /// Narrow to one domain (`code`, `design`, `game`, `security`,
    /// `soft_skills`, `ai`, `ops`). Prerequisites are still evaluated
    /// against the full catalog, so a cross-domain prerequisite still
    /// unlocks correctly.
    #[serde(default)]
    pub domain: Option<String>,
}

use crate::validators::SKILL_DOMAINS as ALLOWED_DOMAINS;

async fn user_skill_tree(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(user_id): Path<Uuid>,
    Query(q): Query<SkillTreeQuery>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(d) = q.domain.as_deref()
        && !ALLOWED_DOMAINS.contains(&d)
    {
        return Err(AppError::Validation(format!(
            "domain must be one of: {}",
            ALLOWED_DOMAINS.join(", ")
        )));
    }

    let is_owner = auth.map(|a| a.user_id) == Some(user_id);
    let readable: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM users
              WHERE id = $1
                AND ($2::BOOLEAN OR (profile_hidden = FALSE AND is_banned = FALSE))
         )",
    )
    .bind(user_id)
    .bind(is_owner)
    .fetch_one(&state.db)
    .await?;
    if !readable {
        return Err(AppError::NotFound(format!("user {user_id} not found")));
    }

    let tree = skill_tree::build_for_user(&state.db, user_id, q.domain.as_deref()).await?;

    // Status counts, so the client can render a summary without walking
    // the tree itself.
    let mut counts = std::collections::BTreeMap::new();
    fn tally(
        nodes: &[skill_tree::SkillTreeNode],
        counts: &mut std::collections::BTreeMap<String, i64>,
    ) {
        for n in nodes {
            *counts.entry(n.status.clone()).or_insert(0) += 1;
            tally(&n.children, counts);
        }
    }
    tally(&tree, &mut counts);

    Ok(Json(wrap(json!({
        "user_id": user_id,
        "tree": tree,
        "counts": counts,
    }))))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetPrerequisitesBody {
    /// Full replacement list. An empty array clears the prerequisites.
    pub prerequisite_skill_ids: Vec<Uuid>,
}

/// Replace a skill's prerequisites.
///
/// PUT rather than PATCH: the body is the complete list, and a partial
/// update of a set that gates the whole tree would be easy to get subtly
/// wrong. Rejected if it would introduce a cycle.
///
/// `AdminGate` only enforces the admin origin and mandatory 2FA; it
/// deliberately does not check the role (see its doc comment). The
/// capability check below is what actually restricts this to admins.
async fn set_prerequisites(
    _gate: AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SetPrerequisitesBody>,
) -> Result<impl IntoResponse, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    if body.prerequisite_skill_ids.len() > 20 {
        return Err(AppError::Validation(
            "at most 20 prerequisites per skill".into(),
        ));
    }
    let updated =
        skill_tree::set_prerequisites(&state.db, id, &body.prerequisite_skill_ids).await?;
    Ok(Json(wrap(json!({
        "skill_id": id,
        "prerequisite_skill_ids": updated,
    }))))
}
