-- A design deliverable is a proof, and the proof table has to be able to say so.
--
-- ## Why this matters more than it looks
--
-- `deliverables` is where proof lives. `ranks` counts verified rows in it,
-- the badge engine reads them, the public portfolio renders them, recruiter
-- search scores them. A validated design challenge that writes no row here
-- moves nothing: the designer sees a green mark and their profile stays
-- exactly as empty as before.
--
-- ## Why one value and not ten
--
-- `artifact_type` already carries four design-shaped values from 0060 —
-- `figma_frame`, `design_tokens_export`, `game_asset`, `blender_asset` — all
-- inherited from the era when design meant screens handed to a developer.
-- None of them describes a brand kit, a sound pack, a typeface or a service
-- blueprint, and adding eight more would rebuild the same mistake at greater
-- length.
--
-- Design deliverables use `design_artifact`, and the shape lives on the slice
-- (`project_slices.design_subtype`, migration 0231). Same decision as the
-- slice type, for the same reason.
--
-- The four legacy values stay: historical rows may use them, and removing a
-- value from a CHECK that existing data relies on is how a migration takes
-- production down.

ALTER TABLE deliverables DROP CONSTRAINT IF EXISTS deliverables_artifact_type_check;

ALTER TABLE deliverables
    ADD CONSTRAINT deliverables_artifact_type_check
    CHECK (artifact_type IN (
        'pr_merged',
        'pr_open',
        'commit',
        'design_artifact',        -- any design deliverable; shape on the slice
        'figma_frame',            -- legacy, superseded by design_artifact
        'design_tokens_export',   -- legacy, superseded by design_artifact
        'playable_build',
        'game_asset',
        'game_scene',
        'cve_report',
        'pentest_writeup',
        'disclosure',
        'code_review',
        'documentation',
        'test_suite',
        'blender_asset',          -- legacy, superseded by design_artifact
        'other'
    ));

COMMENT ON COLUMN deliverables.artifact_type IS
    'What kind of proof this is. Design work is always design_artifact; the '
    'precise shape lives on project_slices.design_subtype.';
