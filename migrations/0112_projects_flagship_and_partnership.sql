-- Content strategy — extend `projects` for flagships and OSS partner curation.
--
-- Rationale (content-strategy-2027-2028.md §4 + Annexe E + Annexe F):
--   The `projects` table (migration 0029) already has `curated_by_admin` and
--   `looking_for_contributors` flags. What's missing to support the content
--   strategy taxonomy:
--
--   1. Flagships (Hello Africa, OpenWeather Africa, future PharmaFinder)
--      - Need a stable long-term steward (mentor senior confirmé)
--      - Need to be visibly distinct from micro-quest OSS partners
--
--   2. OSS partner curation levels (annexe F § levels 1/2/3):
--      - Level 1 = unilateral curation (default)
--      - Level 2 = lightweight partnership (email + label)
--      - Level 3 = formal MoU with the upstream maintainer
--
--   3. Skilluv-side editorial notes (why curated, sensitivity notes,
--      African cultural relevance) — kept internal, not public.

ALTER TABLE projects
    ADD COLUMN is_flagship BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN flagship_steward_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN skilluv_partnership_level SMALLINT
        CHECK (skilluv_partnership_level IS NULL
               OR skilluv_partnership_level BETWEEN 1 AND 3),
    ADD COLUMN skilluv_editorial_notes TEXT;

-- A flagship must have a steward (soft rule: enforceable only when creating
-- via admin API; we allow NULL here for backwards compatibility with any
-- existing rows and for the "steward démissionne mais projet en archivage"
-- transition described in the strategy doc).

-- Index for the discovery routes (talents looking for flagships)
CREATE INDEX idx_projects_flagship
    ON projects (is_flagship)
    WHERE is_flagship = TRUE AND archived_at IS NULL;

-- Index for the OSS partner catalog page (public catalog of curated partners)
CREATE INDEX idx_projects_partnership_level
    ON projects (skilluv_partnership_level, curated_by_admin)
    WHERE skilluv_partnership_level IS NOT NULL AND archived_at IS NULL;

COMMENT ON COLUMN projects.is_flagship IS
    'TRUE for Skilluv flagship projects (long-term, cross-domain, with permanent steward). Examples 2027: Hello Africa, OpenWeather Africa. Not for OSS partners or user projects.';

COMMENT ON COLUMN projects.flagship_steward_user_id IS
    'The permanent steward for this flagship (mentor senior confirmé). Required in application logic for flagships. If the steward démissionne without transferring, the flagship is archived (archived_at set).';

COMMENT ON COLUMN projects.skilluv_partnership_level IS
    'For OSS partners (repos externes curés par Skilluv): 1 = unilateral curation (default), 2 = lightweight partnership (email + label on their side), 3 = formal MoU. NULL = not an OSS partner (regular user project or flagship).';

COMMENT ON COLUMN projects.skilluv_editorial_notes IS
    'Internal editorial notes: why this project is curated, sensitivity notes, African cultural relevance, review guidance for Skilluv mentors. NOT displayed publicly.';
