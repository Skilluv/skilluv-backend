-- Slices that produce a design deliverable.
--
-- ## The same shape as 0181 and 0214, for the same reason
--
-- `slice_type` says which surface the work lives on; `design_subtype` says
-- what the finished artefact is, and is NULL unless the slice is a
-- `design_artifact`.
--
-- ## Two placeholders go
--
-- `figma_frame` and `design_token` were seeded in 0058 and never wired to
-- anything: no ingestion produces them, no service reads them, and
-- `slice_ingestion.rs` mentions `figma_frame` only in a comment about a
-- future ingestor. They also encode the assumption this programme exists to
-- undo — that design means a Figma frame or a token file. One type per file
-- format does not survive twenty-six trades, so both become
-- `design_artifact` with a subtype.
--
-- ## Where the artefact lives
--
-- Wherever the trade already keeps it. A Figma node, a Miro board, a Behance
-- project, an object in our own storage. `design_external_url` is required
-- for the subtypes whose claim is worthless without an address, exactly as
-- 0214 did for model weights: a brand kit nobody can open is a sentence.
--
-- Large binaries — project files, 3D scenes, uncompressed video — are the one
-- case where an address elsewhere is not enough, because the source file has
-- no natural public home. Those upload to our storage, and the column holds
-- the object path. Which one applies is decided by the subtype, not by the
-- uploader.
--
-- ## Which trade it belongs to
--
-- `project_slices.orientation_id` (migration 0186), like every other domain.
-- It is what routes the slice to somebody competent: orientation to
-- `reviewer_group` to `design_reviewer:{group}`. Nothing design-specific is
-- invented for it.

ALTER TABLE project_slices
    DROP CONSTRAINT IF EXISTS project_slices_slice_type_check;

-- Existing rows first: the new constraint would refuse them otherwise.
UPDATE project_slices
   SET slice_type = 'design_artifact'
 WHERE slice_type IN ('figma_frame', 'design_token');

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_slice_type_check
    CHECK (slice_type IN (
        'github_issue', 'game_level', 'game_asset',
        'sec_target', 'cli_task', 'documentation',
        'code_artifact', 'ai_artifact', 'design_artifact',
        'other'
    ));

ALTER TABLE project_slices
    ADD COLUMN design_subtype VARCHAR(30),
    -- Where the artefact lives: a Figma node, a hosted board, a published
    -- project, or an object path in our storage for the formats that have no
    -- public home of their own.
    ADD COLUMN design_external_url TEXT,
    -- What the author says changed since the previous version. Lives on the
    -- slice while the version is current, and is copied into the decision row
    -- when somebody reviews it, so the trail keeps both halves of each round:
    -- what was claimed, and what was found.
    ADD COLUMN design_version_notes_md TEXT,
    -- Plural for the same reason `code_languages` and `ai_frameworks` are: a
    -- brand system delivered as Illustrator sources, an InDesign manual and a
    -- Figma library is one slice, and splitting it would invent artefacts.
    ADD COLUMN design_tools TEXT[] NOT NULL DEFAULT '{}',
    -- How many critique rounds this brief expects, and where it stops. The
    -- ceiling belongs to the brief: a designer deserves to know before
    -- claiming whether this is a one-shot or a conversation.
    ADD COLUMN design_expected_rounds SMALLINT
        CHECK (design_expected_rounds IS NULL
               OR design_expected_rounds BETWEEN 1 AND 5);

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_design_subtype_values
    CHECK (design_subtype IS NULL OR design_subtype IN (
        'interface',        -- screens, flows, a prototype
        'design_system',    -- tokens, components, their documentation
        'brand_kit',        -- marks, palette, type, guidelines
        'illustration_set', -- a set of images and their sources
        'icon_set',         -- an icon system and its delivery formats
        'motion',           -- a motion project and its rendered preview
        'video',            -- a rendered video and its storyboard
        'three_d_scene',    -- a scene, its renders, optionally a glTF
        'sound',            -- audio and its metadata
        'type_family',      -- a typeface and its production files
        'copy_deck',        -- UX writing, naming, verbal guidelines
        'research_document' -- blueprint, journey map, audit, style guide
    ));

-- Rows migrated from the two placeholders, before the coherence constraints
-- go on: `figma_frame` described a product surface and `design_token` a token
-- file, and both are an `interface` deliverable in the new vocabulary. Adding
-- the constraints first would refuse the rows this migration just created.
UPDATE project_slices ps
   SET design_subtype = 'interface',
       orientation_id = COALESCE(ps.orientation_id,
                                 (SELECT id FROM orientations WHERE slug = 'design-product'))
 WHERE ps.slice_type = 'design_artifact' AND ps.design_subtype IS NULL;

-- A subtype only means something on a design artefact, and a design artefact
-- without one is a slice nobody can size, preview or check.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_design_subtype_belongs_to_design_artifact
    CHECK (
        (slice_type = 'design_artifact' AND design_subtype IS NOT NULL)
        OR (slice_type <> 'design_artifact' AND design_subtype IS NULL)
    );

-- A design challenge has to say which trade it is, or nobody competent can be
-- routed to it. The other domains can fall back on `primary_domain`; design
-- cannot, because `design` spans thirteen review families that do not
-- substitute for one another.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_design_artifact_names_its_trade
    CHECK (slice_type <> 'design_artifact' OR orientation_id IS NOT NULL);

COMMENT ON COLUMN project_slices.design_subtype IS
    'What the finished artefact is, for design_artifact slices. slice_type '
    'says which surface the work lives on; this says what comes out of it, '
    'and decides which automatic checks and previews are worth running.';

COMMENT ON COLUMN project_slices.design_external_url IS
    'Where the artefact lives — a Figma node, a hosted board, a published '
    'project, or an object path in our storage for source formats with no '
    'public home.';

COMMENT ON COLUMN project_slices.design_expected_rounds IS
    'How many critique rounds the brief announces. The hard ceiling is five, '
    'enforced on slice_validation_decisions: past it the problem is the brief '
    'or the assignment, not the work.';

CREATE INDEX idx_project_slices_design_subtype
    ON project_slices (design_subtype)
    WHERE design_subtype IS NOT NULL;

CREATE INDEX idx_project_slices_design_tools
    ON project_slices USING gin (design_tools);
