//! SKI-36 / SKI-37 — shared plumbing for the polymorphic "saved items"
//! family: bookmarks and private notes.
//!
//! Both tables key on `(target_type, target_id)` with no foreign key, so
//! this module owns the two invariants a real FK would have given us:
//!
//!   1. **Existence + visibility** — [`assert_target_visible`] resolves
//!      `target_type` to its real table and refuses targets the caller
//!      cannot see anyway (a private deliverable, a hidden profile, an
//!      archived project). Saving something you cannot read would leak
//!      its existence.
//!   2. **Readable output** — [`resolve_labels`] batch-resolves a page of
//!      rows into display labels so the front end does not have to fan out
//!      six extra requests per list.
//!
//! Dangling rows (target hard-deleted after being saved) are never an
//! error: `resolve_labels` simply omits them and the route filters them
//! out, so a deleted target degrades to "the bookmark quietly disappears"
//! rather than a 500.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Every `target_type` accepted by `bookmarks` and `user_notes`. Kept in
/// sync with the CHECK constraints in migrations 0139 / 0140 — the DB is
/// the backstop, this is the fast path that produces a clean 400.
/// Every `target_type` accepted by `bookmarks` and `user_notes`.
///
/// `slice` covers what the ticket called `bounty`: standalone bounties were
/// folded into `project_slices` by migration 0074, where a paid opportunity
/// is a slice carrying a non-zero `credits_reward`.
pub const TARGET_TYPES: &[&str] = &[
    "challenge_template",
    "project",
    "user",
    "team",
    "deliverable",
    "slice",
];

/// Display label for one saved target, resolved from its real table.
#[derive(Debug, Clone, Serialize)]
pub struct TargetLabel {
    pub target_type: String,
    pub target_id: Uuid,
    /// Human-readable title. Falls back to the type name for targets that
    /// have no natural title column (deliverables are identified by their
    /// artifact type).
    pub title: String,
    /// Stable slug or handle when the target has one — lets the front end
    /// build a link without a second lookup. `None` for targets addressed
    /// by UUID only.
    pub slug: Option<String>,
}

/// Reject a `target_type` that is not part of the polymorphic family.
pub fn validate_target_type(target_type: &str) -> Result<(), AppError> {
    if TARGET_TYPES.contains(&target_type) {
        return Ok(());
    }
    Err(AppError::Validation(format!(
        "target_type must be one of: {}",
        TARGET_TYPES.join(", ")
    )))
}

/// Assert the target exists AND is visible to `viewer_id`.
///
/// Visibility rules per type — deliberately matching what the
/// corresponding public read endpoint already enforces:
///
/// * `challenge_template` — must exist and not be archived.
/// * `project` — must exist and not be archived.
/// * `user` — must exist and not be `profile_hidden`; you may always
///   target yourself.
/// * `team` — must exist and not be disbanded.
/// * `deliverable` — must be `public`, or authored by the viewer.
/// * `slice` — must exist and not be a draft.
///
/// Returns [`AppError::NotFound`] (never `Forbidden`) when the target is
/// invisible: distinguishing "does not exist" from "exists but hidden"
/// would turn this endpoint into an existence oracle.
pub async fn assert_target_visible(
    db: &PgPool,
    viewer_id: Uuid,
    target_type: &str,
    target_id: Uuid,
) -> Result<(), AppError> {
    validate_target_type(target_type)?;

    let visible: bool = match target_type {
        "challenge_template" => {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM challenge_templates
                  WHERE id = $1 AND status <> 'archived')",
            )
            .bind(target_id)
            .fetch_one(db)
            .await?
        }
        "project" => {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM projects
                  WHERE id = $1 AND archived_at IS NULL)",
            )
            .bind(target_id)
            .fetch_one(db)
            .await?
        }
        "user" => {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM users
                  WHERE id = $1 AND (profile_hidden = FALSE OR id = $2))",
            )
            .bind(target_id)
            .bind(viewer_id)
            .fetch_one(db)
            .await?
        }
        "team" => {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM challenge_teams
                  WHERE id = $1 AND disbanded_at IS NULL)",
            )
            .bind(target_id)
            .fetch_one(db)
            .await?
        }
        "deliverable" => {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM deliverables
                  WHERE id = $1 AND (public = TRUE OR user_id = $2))",
            )
            .bind(target_id)
            .bind(viewer_id)
            .fetch_one(db)
            .await?
        }
        // Drafts are slices awaiting curator review — not yet public.
        "slice" => {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM project_slices
                  WHERE id = $1 AND status <> 'draft')",
            )
            .bind(target_id)
            .fetch_one(db)
            .await?
        }
        // Unreachable: validate_target_type ran first.
        _ => false,
    };

    if visible {
        Ok(())
    } else {
        Err(AppError::NotFound(format!(
            "{target_type} {target_id} not found"
        )))
    }
}

/// Batch-resolve display labels for a page of saved rows.
///
/// Issues at most one query per distinct `target_type` present in the
/// input (six worst case, one or two in practice) rather than one per
/// row. Targets that no longer exist are simply absent from the result.
pub async fn resolve_labels(
    db: &PgPool,
    targets: &[(String, Uuid)],
) -> Result<HashMap<(String, Uuid), TargetLabel>, AppError> {
    let mut out = HashMap::new();
    if targets.is_empty() {
        return Ok(out);
    }

    // Group ids by type so each type costs exactly one round trip.
    let mut by_type: HashMap<&str, Vec<Uuid>> = HashMap::new();
    for (target_type, target_id) in targets {
        by_type
            .entry(target_type.as_str())
            .or_default()
            .push(*target_id);
    }

    for (target_type, ids) in by_type {
        // (id, title, slug) — each arm projects its table onto that shape.
        let sql = match target_type {
            "challenge_template" => {
                "SELECT id, title, NULL::TEXT FROM challenge_templates WHERE id = ANY($1)"
            }
            "project" => "SELECT id, name, slug::TEXT FROM projects WHERE id = ANY($1)",
            "user" => {
                "SELECT id, COALESCE(NULLIF(display_name, ''), username), username::TEXT
                   FROM users WHERE id = ANY($1)"
            }
            "team" => "SELECT id, name, NULL::TEXT FROM challenge_teams WHERE id = ANY($1)",
            // Deliverables have no title column — the artifact type is the
            // most meaningful short label we can show.
            "deliverable" => {
                "SELECT id, artifact_type, NULL::TEXT FROM deliverables WHERE id = ANY($1)"
            }
            "slice" => "SELECT id, title, NULL::TEXT FROM project_slices WHERE id = ANY($1)",
            // Rows whose type predates a CHECK change: skip rather than fail
            // the whole listing.
            _ => continue,
        };

        let rows: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(&ids)
            .fetch_all(db)
            .await?;

        for (id, title, slug) in rows {
            out.insert(
                (target_type.to_string(), id),
                TargetLabel {
                    target_type: target_type.to_string(),
                    target_id: id,
                    title,
                    slug,
                },
            );
        }
    }

    Ok(out)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn target_type_allowlist_is_enforced() {
        for t in TARGET_TYPES {
            assert!(validate_target_type(t).is_ok(), "{t} should be accepted");
        }
        assert!(validate_target_type("enterprise").is_err());
        assert!(validate_target_type("").is_err());
        // Casing matters — the DB CHECK is case-sensitive too.
        assert!(validate_target_type("User").is_err());
    }
}
