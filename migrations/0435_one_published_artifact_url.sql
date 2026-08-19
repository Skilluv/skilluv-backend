-- One column for where a published artefact lives.
--
-- ## The third column that is not going to exist
--
-- Migration 0181 added `code_package_registry_url` — where a published
-- library can be found. Migration 0214 added `ai_external_hosting_url` —
-- where a published model or dataset can be found, citing 0181. Migration
-- 0231 added `design_external_url` — where a published design artefact can
-- be found, citing 0214. Three columns, one question, and each one written
-- by somebody who had read the previous one.
--
-- Ops publishes too: a Terraform module on the registry, a collection on
-- Galaxy, a chart on ArtifactHub, an image on Docker Hub. A fourth column
-- would have made the sweep's COALESCE four wide, and the constraint that
-- says "a published thing says where" would have been written a fourth time.
--
-- So: one column, and the three domain-specific ones go. The same argument
-- migration 0306 made about onboarding answers, on a smaller table.
--
-- ## What the column is allowed to hold
--
-- A public address, usually. Design is the exception 0231 named: source
-- formats with no public home — project files, 3D scenes, uncompressed video
-- — upload to our storage and the column holds the object path. Nothing
-- downstream breaks on that: the registry reader returns "not a registry I
-- know" for a path, which is the truth, and no figures are fetched.
--
-- ## Why the checks stay per domain
--
-- What is published differs: a `library_published` code slice must name a
-- registry, an `ml_model` must name a host, an `iac_terraform` artefact need
-- not be published at all — a module written for one client is still an
-- artefact. The column is shared; the obligation is not, and each domain
-- keeps its own.

ALTER TABLE project_slices
    RENAME COLUMN code_package_registry_url TO published_artifact_url;

-- Nothing is lost: the two were mutually exclusive in practice, and the
-- COALESCE in the sweep is the proof — it would have picked the code one and
-- ignored the AI one on any row that had both.
UPDATE project_slices
   SET published_artifact_url = ai_external_hosting_url
 WHERE published_artifact_url IS NULL
   AND ai_external_hosting_url IS NOT NULL;

-- Said out loud rather than assumed. If a row had both and they differed,
-- the AI one is now unreachable, and somebody should know before the column
-- goes rather than after.
DO $$
DECLARE
    conflicting INTEGER;
BEGIN
    SELECT count(*) INTO conflicting
      FROM project_slices
     WHERE ai_external_hosting_url IS NOT NULL
       AND published_artifact_url IS NOT NULL
       AND published_artifact_url <> ai_external_hosting_url;

    IF conflicting > 0 THEN
        RAISE WARNING
            'ail % slice(s) carried two different published URLs; the code one was kept',
            conflicting;
    END IF;
END $$;

ALTER TABLE project_slices DROP COLUMN ai_external_hosting_url;

UPDATE project_slices
   SET published_artifact_url = design_external_url
 WHERE published_artifact_url IS NULL
   AND design_external_url IS NOT NULL;

ALTER TABLE project_slices DROP COLUMN design_external_url;

COMMENT ON COLUMN project_slices.published_artifact_url IS
    'Where the finished artefact can be found by somebody who wants to use '
    'it — a package registry, a model hub, an infrastructure registry, a '
    'container registry. Skilluv does not host any of it: these things have '
    'free homes where the people who would use them already look.';

-- The two constraints named their own columns, so both are restated.

ALTER TABLE project_slices
    DROP CONSTRAINT IF EXISTS project_slices_published_library_has_a_registry;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_published_library_has_a_registry
    CHECK (
        code_subtype IS DISTINCT FROM 'library_published'
        OR published_artifact_url IS NOT NULL
    );

ALTER TABLE project_slices
    DROP CONSTRAINT IF EXISTS project_slices_hosted_ai_artifact_says_where;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_hosted_ai_artifact_says_where
    CHECK (
        ai_subtype IS NULL
        OR ai_subtype NOT IN ('ml_model', 'dataset', 'ai_research_paper',
                              'ai_service_api')
        OR published_artifact_url IS NOT NULL
    );

-- Ops adds no obligation of its own, and the absence is the position: a
-- module written for one client and never published is still an artefact,
-- and a review grid judges whether somebody else can run it, not whether it
-- is on a registry. Publishing is what makes the figures fetchable, not what
-- makes the work count.

CREATE INDEX idx_project_slices_published_artifact_url
    ON project_slices (published_artifact_url)
    WHERE published_artifact_url IS NOT NULL;
