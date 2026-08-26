-- Real security work, on the slice table, with the one column the domain
-- cannot do without.
--
-- ## What a security slice is, now that 0549 has taken the other half
--
-- 0549 put the practice catalogue where it belongs: flags, labs and
-- walkthroughs are challenges, because the answer was planted and hundreds of
-- people will look for it. What is left is the work where nobody knows whether
-- there is an answer — an audit of a real repository, a scoped hunt on a live
-- deployment, a threat model of an architecture that exists, a policy set
-- somebody has to be audited on. That is a slice: claimed once, done once,
-- read by a human, and it produces a deliverable.
--
-- ## `sec_target` is replaced rather than kept
--
-- The domain had one slice type, `sec_target` — "Cible d'audit" — seeded by
-- 0413 with no subtype column and no rows ever created against it. Two things
-- are wrong with it: the name says *target*, which fits a hunt and does not
-- fit a threat model or a policy review, and it is the only slice type left
-- whose display name is in French while every domain added since names them in
-- English.
--
-- It is deleted and replaced by `security_artifact`, which is the name every
-- other domain uses for the same thing. The delete is guarded: if any slice
-- ever referenced it, the migration stops rather than orphaning it.
--
-- ## The column this domain cannot do without
--
-- `security_authorisation_url`. Every other domain's slice can be started by
-- reading the description; a security slice cannot legally be started without
-- a written permission that names what may be touched. Making it a required
-- column, refused by a CHECK, is the difference between a platform that says
-- "always get authorisation" in a charter and one where an unauthorised piece
-- of work cannot be created.
--
-- It is a URL rather than prose because the authorisation has to be the same
-- document for everybody working under it — the published scope, the rules of
-- engagement attached to a mission, the bug bounty policy of a project. A
-- paragraph pasted into a description is a copy that drifts.

-- ═══════════════════════════════════════════════════════════════════
-- The slice type
-- ═══════════════════════════════════════════════════════════════════

DO $$
DECLARE
    orphans INT;
BEGIN
    SELECT count(*) INTO orphans FROM project_slices WHERE slice_type = 'sec_target';
    IF orphans > 0 THEN
        RAISE EXCEPTION
            'sec_target carries % slices — rename it in place instead of replacing it',
            orphans;
    END IF;
END $$;

INSERT INTO slice_types (slug, skill_domain, name, description, sort_order) VALUES
('security_artifact', 'security', 'Security artefact',
 'Work on a system that exists, under a written authorisation: an audit, a '
 'scoped hunt, a threat model, a policy set. Read by a person, because nobody '
 'planted the answer.', 90);

DELETE FROM slice_types WHERE slug = 'sec_target';

-- ═══════════════════════════════════════════════════════════════════
-- What a security slice carries
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE project_slices
    ADD COLUMN security_subtype VARCHAR(25)
        CHECK (security_subtype IS NULL OR security_subtype IN (
            'finding_hunt',          -- scoped hunting on a live target
            'code_audit',            -- reading a codebase for exploitable defects
            'threat_model',          -- an architecture, before the code
            'governance_review',     -- policies, controls, evidence
            'detection_engineering', -- rules built and validated
            'purple_exercise',       -- attack and defence in one exercise
            'incident_analysis'      -- a real incident, written up
        )),
    -- The written permission. Not optional for this domain, enforced below.
    ADD COLUMN security_authorisation_url VARCHAR(500)
        CHECK (security_authorisation_url IS NULL
               OR security_authorisation_url ~ '^(https://|/)'),
    -- What is being worked on, when it is a host rather than a repository.
    -- A repository is `projects.github_repo_*` and does not need repeating.
    ADD COLUMN security_target_host VARCHAR(255),
    -- Which frameworks a governance slice answers to. Mirrors
    -- `missions.compliance_frameworks`, which 0424 added for the same reason:
    -- "ISO 27001" and "SOC 2" are what the work is judged against, and a
    -- reviewer has to know which before reading a word.
    ADD COLUMN security_frameworks TEXT[] NOT NULL DEFAULT '{}';

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_security_subtype_belongs CHECK (
        (slice_type = 'security_artifact') = (security_subtype IS NOT NULL)
    ),
    -- The whole point of the column.
    ADD CONSTRAINT security_work_is_authorised CHECK (
        slice_type <> 'security_artifact' OR security_authorisation_url IS NOT NULL
    ),
    -- A hunt names what it is hunting on. An audit does not have to: its
    -- target is the project.
    ADD CONSTRAINT a_hunt_names_its_target CHECK (
        security_subtype <> 'finding_hunt' OR security_target_host IS NOT NULL
    ),
    -- A governance review names its framework, for the same reason a hunt
    -- names its host: without it there is nothing to review against.
    ADD CONSTRAINT a_governance_review_names_its_framework CHECK (
        security_subtype <> 'governance_review'
        OR cardinality(security_frameworks) > 0
    ),
    -- Frameworks belong to security work. Elsewhere the array stays empty.
    ADD CONSTRAINT security_frameworks_belong_to_security CHECK (
        cardinality(security_frameworks) = 0 OR primary_domain = 'security'
    );

COMMENT ON COLUMN project_slices.security_authorisation_url IS
    'The written permission this work is done under. Required by a CHECK, not '
    'by a charter: an unauthorised piece of security work should not be '
    'creatable, and every other layer of this platform would have let it be.';

COMMENT ON COLUMN project_slices.security_subtype IS
    'Which security trade the artefact belongs to. Practice work — flags, '
    'labs, walkthroughs — is not here: it lives in challenge_templates, '
    'because a slice is claimed once and those are meant to be solved by '
    'everybody (0549).';

CREATE INDEX idx_project_slices_security
    ON project_slices (security_subtype, status)
    WHERE security_subtype IS NOT NULL;
