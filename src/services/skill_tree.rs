//! SKI-47 (Post-MVP T3-04) — skill tree with prerequisites.
//!
//! Builds the whole tree in one pass: a single query loads every skill
//! node plus the caller's proficiency, and the traversal happens in
//! memory. The alternative — recursing per node — would issue a query per
//! edge for a payload the front end renders all at once.
//!
//! ## Node status
//!
//! | status        | meaning                                              |
//! |---------------|------------------------------------------------------|
//! | `locked`      | at least one prerequisite is unproven                 |
//! | `unlocked`    | prerequisites met, nothing proven on this skill yet   |
//! | `in_progress` | proven at least once, below the top proficiency level |
//! | `mastered`    | proficiency level 5                                   |
//!
//! The ticket named the first three. `mastered` is split out of
//! `in_progress` because a level-5 skill described as "in progress" reads
//! as unfinished work on a profile, which is the opposite of what it is.
//!
//! A prerequisite counts as met once the skill has been proven at all
//! (`proven_count >= 1`), not at some proficiency threshold: the gate is
//! "have you done this", and requiring level 3 in Blender before touching
//! Godot would lock the tree far harder than any curriculum intends.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const STATUS_LOCKED: &str = "locked";
pub const STATUS_UNLOCKED: &str = "unlocked";
pub const STATUS_IN_PROGRESS: &str = "in_progress";
pub const STATUS_MASTERED: &str = "mastered";

/// Depth cap for the hierarchical view.
///
/// The taxonomy is a handful of levels deep in practice. The cap exists so
/// a cycle written directly into the database — bypassing
/// [`assert_no_cycle`] — produces a truncated tree instead of an infinite
/// loop in a request handler.
const MAX_DEPTH: usize = 16;

/// One node of the tree, with the caller's progress on it.
#[derive(Debug, Clone, Serialize)]
pub struct SkillTreeNode {
    pub id: Uuid,
    pub slug: String,
    pub display_name: String,
    pub domain: String,
    pub display_category: String,
    pub parent_id: Option<Uuid>,
    pub prerequisite_skill_ids: Vec<Uuid>,
    /// Prerequisites the user has not proven yet — what to show on hover
    /// for a locked node.
    pub missing_prerequisites: Vec<MissingPrerequisite>,
    pub status: String,
    pub proven_count: i32,
    pub proficiency_level: i16,
    /// Taxonomy children, recursively.
    pub children: Vec<SkillTreeNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissingPrerequisite {
    pub id: Uuid,
    pub slug: String,
    pub display_name: String,
}

/// Flat row as loaded from the database.
#[derive(Debug, Clone, sqlx::FromRow)]
struct SkillRow {
    id: Uuid,
    slug: String,
    display_name: String,
    domain: String,
    display_category: String,
    parent_id: Option<Uuid>,
    prerequisite_skill_ids: Vec<Uuid>,
    proven_count: Option<i32>,
    proficiency_level: Option<i16>,
}

/// Build a user's skill tree.
///
/// `domain` optionally narrows to one domain (`code`, `game`, ...). Nodes
/// are returned as taxonomy roots with nested children.
pub async fn build_for_user(
    db: &PgPool,
    user_id: Uuid,
    domain: Option<&str>,
) -> Result<Vec<SkillTreeNode>, AppError> {
    // One query: every skill, LEFT JOINed onto this user's progress.
    let rows: Vec<SkillRow> = sqlx::query_as(
        r#"
        SELECT sn.id,
               sn.slug,
               sn.display_name,
               sn.domain,
               sn.display_category,
               sn.parent_id,
               sn.prerequisite_skill_ids,
               us.proven_count,
               us.proficiency_level
          FROM skill_nodes sn
          LEFT JOIN user_skills us
                 ON us.skill_id = sn.id AND us.user_id = $1
         WHERE ($2::TEXT IS NULL OR sn.domain = $2)
         ORDER BY sn.display_name ASC
        "#,
    )
    .bind(user_id)
    .bind(domain)
    .fetch_all(db)
    .await?;

    // Proven set: computed over ALL skills, never over the filtered subset.
    // A prerequisite may sit in another domain (godot-3d requires
    // blender-basics, which is `design`), so filtering first would make it
    // look unproven and lock the node.
    let proven: HashSet<Uuid> = if domain.is_some() {
        sqlx::query_scalar(
            "SELECT skill_id FROM user_skills WHERE user_id = $1 AND proven_count >= 1",
        )
        .bind(user_id)
        .fetch_all(db)
        .await?
        .into_iter()
        .collect()
    } else {
        rows.iter()
            .filter(|r| r.proven_count.unwrap_or(0) >= 1)
            .map(|r| r.id)
            .collect()
    };

    // Names for prerequisites, which may point outside the filtered set.
    let names: HashMap<Uuid, (String, String)> = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT id, slug, display_name FROM skill_nodes",
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|(id, slug, name)| (id, (slug, name)))
    .collect();

    // Flat nodes first, then assembled into the hierarchy.
    let mut flat: HashMap<Uuid, SkillTreeNode> = HashMap::with_capacity(rows.len());
    let mut order: Vec<Uuid> = Vec::with_capacity(rows.len());

    for r in rows {
        let proven_count = r.proven_count.unwrap_or(0);
        let proficiency_level = r.proficiency_level.unwrap_or(0);

        let missing: Vec<MissingPrerequisite> = r
            .prerequisite_skill_ids
            .iter()
            .filter(|p| !proven.contains(p))
            .map(|p| {
                let (slug, display_name) = names
                    .get(p)
                    .cloned()
                    // A prerequisite pointing at a deleted skill: surface it
                    // rather than dropping it, so the gap is visible instead
                    // of silently unlocking the node.
                    .unwrap_or_else(|| ("unknown".to_string(), "Unknown skill".to_string()));
                MissingPrerequisite {
                    id: *p,
                    slug,
                    display_name,
                }
            })
            .collect();

        let status = if !missing.is_empty() {
            STATUS_LOCKED
        } else if proven_count < 1 {
            STATUS_UNLOCKED
        } else if proficiency_level >= 5 {
            STATUS_MASTERED
        } else {
            STATUS_IN_PROGRESS
        };

        order.push(r.id);
        flat.insert(
            r.id,
            SkillTreeNode {
                id: r.id,
                slug: r.slug,
                display_name: r.display_name,
                domain: r.domain,
                display_category: r.display_category,
                parent_id: r.parent_id,
                prerequisite_skill_ids: r.prerequisite_skill_ids,
                missing_prerequisites: missing,
                status: status.to_string(),
                proven_count,
                proficiency_level,
                children: Vec::new(),
            },
        );
    }

    // Assemble bottom-up. Children are collected per parent, then each
    // parent is rebuilt from the leaves upward so ownership moves once.
    let mut children_of: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let mut roots: Vec<Uuid> = Vec::new();
    for id in &order {
        let parent = flat[id].parent_id;
        // A node whose parent was filtered out by the domain narrowing is
        // a root of the returned view, not an orphan to drop.
        match parent.filter(|p| flat.contains_key(p)) {
            Some(p) => children_of.entry(p).or_default().push(*id),
            None => roots.push(*id),
        }
    }

    Ok(roots
        .into_iter()
        .map(|r| assemble(r, &mut flat, &children_of, 0))
        .collect())
}

/// Move `id` out of `flat`, attaching its children recursively.
///
/// Depth-capped: past [`MAX_DEPTH`] the subtree is returned childless
/// rather than recursing forever on a cycle.
fn assemble(
    id: Uuid,
    flat: &mut HashMap<Uuid, SkillTreeNode>,
    children_of: &HashMap<Uuid, Vec<Uuid>>,
    depth: usize,
) -> SkillTreeNode {
    let mut node = flat.remove(&id).expect("node is assembled exactly once");
    if depth >= MAX_DEPTH {
        return node;
    }
    if let Some(kids) = children_of.get(&id) {
        // Collected first so the `flat` borrow from the filter ends before
        // the recursive calls take it mutably. `flat.remove` above means a
        // node already claimed by another parent is skipped, so a parent
        // cycle cannot duplicate work.
        let pending: Vec<Uuid> = kids
            .iter()
            .copied()
            .filter(|k| flat.contains_key(k))
            .collect();
        node.children = pending
            .into_iter()
            .map(|k| assemble(k, flat, children_of, depth + 1))
            .collect();
    }
    node
}

/// Reject a prerequisite list that would introduce a cycle.
///
/// Walks the prerequisite graph from each proposed prerequisite; if the
/// walk reaches `skill_id`, adding the edge would close a loop and every
/// skill on it would be permanently locked.
pub async fn assert_no_cycle(
    db: &PgPool,
    skill_id: Uuid,
    proposed: &[Uuid],
) -> Result<(), AppError> {
    if proposed.contains(&skill_id) {
        return Err(AppError::Validation(
            "a skill cannot be its own prerequisite".into(),
        ));
    }
    if proposed.is_empty() {
        return Ok(());
    }

    // Every prerequisite must exist — a dangling id would lock the node
    // against a skill that can never be proven.
    let existing: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM skill_nodes WHERE id = ANY($1)")
        .bind(proposed)
        .fetch_all(db)
        .await?;
    if existing.len() != proposed.len() {
        let known: HashSet<Uuid> = existing.into_iter().collect();
        let missing: Vec<String> = proposed
            .iter()
            .filter(|p| !known.contains(p))
            .map(|p| p.to_string())
            .collect();
        return Err(AppError::NotFound(format!(
            "unknown prerequisite skill(s): {}",
            missing.join(", ")
        )));
    }

    // Breadth-first over the existing graph, treating the proposed edges as
    // if they were already in place.
    let mut seen: HashSet<Uuid> = HashSet::new();
    let mut frontier: Vec<Uuid> = proposed.to_vec();

    for _ in 0..MAX_DEPTH {
        if frontier.is_empty() {
            return Ok(());
        }
        if frontier.contains(&skill_id) {
            return Err(AppError::Validation(
                "these prerequisites would create a cycle — the skills involved \
                 could never be unlocked"
                    .into(),
            ));
        }
        let fresh: Vec<Uuid> = frontier
            .iter()
            .copied()
            .filter(|f| seen.insert(*f))
            .collect();
        if fresh.is_empty() {
            return Ok(());
        }
        frontier = sqlx::query_scalar::<_, Vec<Uuid>>(
            "SELECT prerequisite_skill_ids FROM skill_nodes WHERE id = ANY($1)",
        )
        .bind(&fresh)
        .fetch_all(db)
        .await?
        .into_iter()
        .flatten()
        .collect();
    }

    // Ran out of budget without settling: refuse rather than accept an
    // edge we could not prove safe.
    Err(AppError::Validation(
        "prerequisite chain is too deep to validate".into(),
    ))
}

/// A skill's current prerequisites.
///
/// Read separately from `set_prerequisites` rather than returned by it,
/// because the caller that needs the previous list — the audit entry on
/// the admin PUT — needs it even when the replacement is rejected.
pub async fn prerequisites_of(db: &PgPool, skill_id: Uuid) -> Result<Vec<Uuid>, AppError> {
    let current: Option<Vec<Uuid>> =
        sqlx::query_scalar("SELECT prerequisite_skill_ids FROM skill_nodes WHERE id = $1")
            .bind(skill_id)
            .fetch_optional(db)
            .await?;
    current.ok_or_else(|| AppError::NotFound(format!("skill {skill_id} not found")))
}

/// Replace a skill's prerequisites, after the cycle check.
pub async fn set_prerequisites(
    db: &PgPool,
    skill_id: Uuid,
    prerequisites: &[Uuid],
) -> Result<Vec<Uuid>, AppError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM skill_nodes WHERE id = $1)")
        .bind(skill_id)
        .fetch_one(db)
        .await?;
    if !exists {
        return Err(AppError::NotFound(format!("skill {skill_id} not found")));
    }

    // Deduplicate: the same prerequisite listed twice is one prerequisite.
    let mut seen = HashSet::new();
    let unique: Vec<Uuid> = prerequisites
        .iter()
        .copied()
        .filter(|p| seen.insert(*p))
        .collect();

    assert_no_cycle(db, skill_id, &unique).await?;

    let updated: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE skill_nodes SET prerequisite_skill_ids = $2, updated_at = NOW()
          WHERE id = $1
          RETURNING prerequisite_skill_ids",
    )
    .bind(skill_id)
    .bind(&unique)
    .fetch_one(db)
    .await?;

    Ok(updated)
}

#[cfg(test)]
mod unit {
    use super::*;

    fn row(
        id: Uuid,
        parent: Option<Uuid>,
        prereqs: Vec<Uuid>,
        proven: i32,
        level: i16,
    ) -> SkillRow {
        SkillRow {
            id,
            slug: format!("s{}", &id.to_string()[..4]),
            display_name: "Skill".into(),
            domain: "code".into(),
            display_category: "craft".into(),
            parent_id: parent,
            prerequisite_skill_ids: prereqs,
            proven_count: Some(proven),
            proficiency_level: Some(level),
        }
    }

    /// Mirror of the status ladder in `build_for_user`, so the transitions
    /// can be asserted without a database.
    fn status_for(missing: bool, proven: i32, level: i16) -> &'static str {
        if missing {
            STATUS_LOCKED
        } else if proven < 1 {
            STATUS_UNLOCKED
        } else if level >= 5 {
            STATUS_MASTERED
        } else {
            STATUS_IN_PROGRESS
        }
    }

    #[test]
    fn status_ladder_covers_every_transition() {
        assert_eq!(status_for(true, 0, 0), STATUS_LOCKED);
        // A missing prerequisite outranks any amount of progress: the node
        // should not read as available.
        assert_eq!(status_for(true, 9, 5), STATUS_LOCKED);
        assert_eq!(status_for(false, 0, 0), STATUS_UNLOCKED);
        assert_eq!(status_for(false, 1, 1), STATUS_IN_PROGRESS);
        assert_eq!(status_for(false, 30, 4), STATUS_IN_PROGRESS);
        assert_eq!(status_for(false, 30, 5), STATUS_MASTERED);
    }

    #[test]
    fn assemble_nests_children_and_stops_at_the_depth_cap() {
        // A chain longer than MAX_DEPTH, which is what a cycle written
        // straight into the database would look like to the traversal.
        let ids: Vec<Uuid> = (0..MAX_DEPTH + 5).map(|_| Uuid::new_v4()).collect();
        let mut flat = HashMap::new();
        let mut children_of: HashMap<Uuid, Vec<Uuid>> = HashMap::new();

        for (i, id) in ids.iter().enumerate() {
            let parent = if i == 0 { None } else { Some(ids[i - 1]) };
            let r = row(*id, parent, vec![], 0, 0);
            if let Some(p) = parent {
                children_of.entry(p).or_default().push(*id);
            }
            flat.insert(
                *id,
                SkillTreeNode {
                    id: r.id,
                    slug: r.slug,
                    display_name: r.display_name,
                    domain: r.domain,
                    display_category: r.display_category,
                    parent_id: r.parent_id,
                    prerequisite_skill_ids: r.prerequisite_skill_ids,
                    missing_prerequisites: Vec::new(),
                    status: STATUS_UNLOCKED.into(),
                    proven_count: 0,
                    proficiency_level: 0,
                    children: Vec::new(),
                },
            );
        }

        let tree = assemble(ids[0], &mut flat, &children_of, 0);

        // Walk down and count: the traversal must terminate.
        let mut depth = 0;
        let mut cursor = &tree;
        while let Some(child) = cursor.children.first() {
            depth += 1;
            cursor = child;
        }
        assert_eq!(
            depth, MAX_DEPTH,
            "traversal stops at the cap instead of recursing forever"
        );
    }
}
