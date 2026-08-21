-- Which trade a piece of work belongs to.
--
-- ## The gap
--
-- A slice knew its domain and nothing narrower. So the platform could say
-- somebody had done code, and not that they had done kernel work — which is
-- the whole point of naming thirty-three trades in migration 0173.
--
-- It showed up concretely in migration 0177: the "worked in three different
-- code trades" badge had to be granted by hand, because a deliverable points
-- at a challenge, a challenge carries a domain, and nothing carried a trade.
-- With this column it is countable, and that badge becomes derived like the
-- others.
--
-- ## Why the label mapping is a table
--
-- Upstream projects label their issues their own way: `E-easy` on Rust
-- repos, `good first issue` almost everywhere, `area/frontend` on some. The
-- ingestion cannot guess which trade a label means, and hard-coding a
-- dictionary would mean a deployment every time a partner renames a label.
--
-- One row per (project, label) says it, and an operator maintains it in the
-- admin panel next to the project it belongs to.

ALTER TABLE project_slices
    ADD COLUMN orientation_id UUID REFERENCES orientations(id) ON DELETE SET NULL;

COMMENT ON COLUMN project_slices.orientation_id IS
    'The trade this slice belongs to. NULL when nothing said — an unlabelled '
    'issue is honestly untyped, and guessing would put people in trades they '
    'never worked in.';

CREATE INDEX idx_project_slices_orientation
    ON project_slices (orientation_id)
    WHERE orientation_id IS NOT NULL;

CREATE TABLE project_label_orientations (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- As the upstream repository spells it, case included. Matching is done
    -- on the exact string GitHub sends.
    label VARCHAR(120) NOT NULL,
    orientation_id UUID NOT NULL REFERENCES orientations(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project_id, label)
);

COMMENT ON TABLE project_label_orientations IS
    'What an upstream label means, per project. Rust repos say E-easy, others '
    'say good first issue; the ingestion cannot guess, and a dictionary in '
    'the binary would need a deployment every time a partner renames one.';

CREATE INDEX idx_project_label_orientations_orientation
    ON project_label_orientations (orientation_id);

-- ═══════════════════════════════════════════════════════════════════
-- A trade that was renamed keeps pointing somewhere useful
-- ═══════════════════════════════════════════════════════════════════
--
-- The catalogue an operator writes these from was drafted against the old
-- slugs. Rather than refuse a mapping that names `dev-frontend`, follow the
-- lineage recorded in 0173 — which is what `replaced_by` was added for.

CREATE OR REPLACE FUNCTION resolve_orientation(wanted_slug TEXT)
RETURNS UUID AS $$
    SELECT COALESCE(o.replaced_by, o.id)
      FROM orientations o
     WHERE o.slug = wanted_slug;
$$ LANGUAGE sql STABLE;

COMMENT ON FUNCTION resolve_orientation(TEXT) IS
    'The live orientation behind a slug, following one rename. Lets a seed '
    'written against the old catalogue land on the trade that replaced it '
    'instead of failing or pointing at something archived.';
