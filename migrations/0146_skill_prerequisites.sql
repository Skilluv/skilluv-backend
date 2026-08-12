-- SKI-47 (Post-MVP T3-04) — skill prerequisites for the skill tree.
--
-- `skill_nodes.parent_id` (migration 0056) already models *taxonomy*:
-- "godot-3d belongs under game-engines". Prerequisites model *order*:
-- "godot-3d requires blender-basics". Those are different relations —
-- a prerequisite frequently lives in another branch of the taxonomy
-- entirely (react requires javascript; both sit under `code`, but neither
-- is the other's parent) — so this is a new column rather than a reuse of
-- parent_id.
--
-- ## Why an array and not a join table
--
-- The relation is read as a whole, always: rendering a skill tree needs
-- every node's full prerequisite list in one pass. A join table would mean
-- a second query and a regroup for data that is never queried by its other
-- side ("which skills require X" is not a screen). An array keeps the tree
-- endpoint at one SELECT.
--
-- ## Cycle prevention
--
-- A CHECK can only see the row in front of it, so it can rule out the
-- direct self-reference and nothing more. Longer cycles (A -> B -> A) are
-- rejected by `services::skill_tree::assert_no_cycle` on write, and the
-- read path is depth-capped regardless, so even a cycle introduced by a
-- direct database edit degrades to a truncated tree rather than a hang.

ALTER TABLE skill_nodes
    ADD COLUMN IF NOT EXISTS prerequisite_skill_ids UUID[] NOT NULL DEFAULT '{}';

-- The one cycle a CHECK can catch on its own.
ALTER TABLE skill_nodes
    DROP CONSTRAINT IF EXISTS skill_nodes_no_self_prerequisite;
ALTER TABLE skill_nodes
    ADD CONSTRAINT skill_nodes_no_self_prerequisite
    CHECK (NOT (id = ANY(prerequisite_skill_ids)));

-- GIN index so "which skills list X as a prerequisite" stays cheap for the
-- cycle check on write.
CREATE INDEX IF NOT EXISTS idx_skill_nodes_prerequisites
    ON skill_nodes USING GIN (prerequisite_skill_ids);
