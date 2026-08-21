-- Slices that produce something other than a merged issue.
--
-- ## What was missing
--
-- `slice_type` had `github_issue` and eight siblings, all describing where
-- the work happens. None describes a published library, an accepted RFC or a
-- benchmark — artefacts the code trades produce constantly and that the
-- platform had no way to represent, so they were filed as `other` and lost
-- their shape.
--
-- ## Why a subtype and not nine more slice types
--
-- `slice_type` says which surface the work lives on, and the ingestion, the
-- validation queues and the UI all switch on it. Adding seven values there
-- would mean seven new branches in every one of those, for distinctions that
-- only matter once the work is done and being attested.
--
-- `code_subtype` says what the finished artefact is. It is NULL unless the
-- slice is a `code_artifact`, and the constraint enforces that: a subtype on
-- a Figma frame would be a claim nothing reads.

ALTER TABLE project_slices
    DROP CONSTRAINT IF EXISTS project_slices_slice_type_check;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_slice_type_check
    CHECK (slice_type IN (
        'github_issue', 'figma_frame', 'game_level', 'game_asset',
        'sec_target', 'cli_task', 'design_token', 'documentation',
        'code_artifact',
        'other'
    ));

ALTER TABLE project_slices
    ADD COLUMN code_subtype VARCHAR(30),
    -- Plural on purpose: a slice that touches Rust and TypeScript is one
    -- slice. Forcing a single language would either lose half the work or
    -- split it into two artefacts that were never separate.
    ADD COLUMN code_languages TEXT[] NOT NULL DEFAULT '{}',
    -- The repository the work landed in, when it is not ours. Distinct from
    -- `fork_repo_url`, which is where the contributor worked.
    ADD COLUMN code_external_repo_url TEXT,
    -- Where a published package can be found and its downloads counted.
    ADD COLUMN code_package_registry_url TEXT;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_code_subtype_values
    CHECK (code_subtype IS NULL OR code_subtype IN (
        'pr_upstream',
        'library_published',
        'application_shipped',
        'rfc_document',
        'devtool_authored',
        'benchmark_result',
        'security_disclosure'
    ));

-- A subtype only means something on a code artefact, and a code artefact
-- without one is a slice nobody can attest against.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_code_subtype_belongs_to_code_artifact
    CHECK (
        (slice_type = 'code_artifact' AND code_subtype IS NOT NULL)
        OR (slice_type <> 'code_artifact' AND code_subtype IS NULL)
    );

-- A published library has to say where it was published. Without the URL the
-- claim cannot be checked against the registry, which is the only thing that
-- makes it more than a sentence.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_published_library_has_a_registry
    CHECK (
        code_subtype IS DISTINCT FROM 'library_published'
        OR code_package_registry_url IS NOT NULL
    );

COMMENT ON COLUMN project_slices.code_subtype IS
    'What the finished artefact is, for code_artifact slices. slice_type says '
    'which surface the work lives on; this says what comes out of it.';

COMMENT ON COLUMN project_slices.code_languages IS
    'Every language the slice touches. Plural because a slice that spans two '
    'is one slice, and splitting it would invent an artefact.';

CREATE INDEX idx_project_slices_code_subtype
    ON project_slices (code_subtype)
    WHERE code_subtype IS NOT NULL;

CREATE INDEX idx_project_slices_code_languages
    ON project_slices USING gin (code_languages);
