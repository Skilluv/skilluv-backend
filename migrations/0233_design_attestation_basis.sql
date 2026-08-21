-- What a design attestation rests on.
--
-- The design backlog asked for seven new `attestation_type` values —
-- `contest_winner`, `brand_delivered`, `featured_designer` and so on.
-- Migrations 0178 and 0213 settled that argument twice already, and the
-- reasoning holds a third time: the three types say what *kind* of thing is
-- attested, and each carries an invariant. "Won a contest" is not a kind; it
-- is the evidence, and it can support a skill attestation or a compagnonnage
-- one. Putting it on `attestation_type` would mix two axes and leave the
-- invariants with no branch to attach to.
--
-- So: seven values on `basis`. Nothing new is invented for design.
--
-- ## Six of the seven point at something
--
-- A brand system delivered, a typeface released, a contest won — each rests
-- on an artefact a stranger can open, and each has a `deliverables` row by
-- the time the attestation is issued. The constraint requires the link, so
-- the basis is a claim that can be checked rather than a label.
--
-- `featured_designer` is the exception, for the same reason
-- `featured_coder` and `featured_ai_researcher` are: it is an editorial
-- decision about a person, not a claim about one artefact.
--
-- ## Why a validated challenge is a basis at all
--
-- In code, the basis worth naming is the merge upstream — the platform's
-- validation is the ordinary path and needs no special mention. In design
-- there is no upstream: the validation *is* the outcome, reached after a
-- critique conversation somebody can read. Naming it makes the ordinary path
-- attestable, which is the difference between a designer leaving with a
-- profile and leaving with a certificate.

ALTER TABLE attestations
    DROP CONSTRAINT IF EXISTS attestations_basis_check;

ALTER TABLE attestations
    ADD CONSTRAINT attestations_basis_check
    CHECK (basis IS NULL OR basis IN (
        -- Code (migration 0178)
        'code_pr_merged_upstream',
        'code_project_shipped',
        'code_library_published',
        'code_rfc_accepted',
        'code_standard_contribution',
        'code_devtool_adopted',
        'featured_coder',
        -- AI (migration 0213)
        'ai_model_shipped',
        'ai_dataset_published',
        'ai_agent_system_deployed',
        'ai_paper_published',
        'ai_benchmark_result',
        'ai_safety_finding_validated',
        'featured_ai_researcher',
        -- Design. Each names something a stranger can open.
        'design_deliverable_validated',   -- a challenge validated after critique
        'design_brand_system_delivered',  -- a complete identity and its guidelines
        'design_typeface_released',       -- a family published with its production files
        'design_system_adopted',          -- a system another team builds on
        'design_contest_won',             -- a podium finish in a design contest
        'design_mission_delivered',       -- a paid mission accepted by the client
        'featured_designer'
    ));

ALTER TABLE attestations
    DROP CONSTRAINT IF EXISTS attestations_artifact_basis_links_a_deliverable;

ALTER TABLE attestations
    ADD CONSTRAINT attestations_artifact_basis_links_a_deliverable
    CHECK (
        basis IS NULL
        OR basis NOT IN ('code_pr_merged_upstream', 'code_project_shipped',
                         'code_library_published',
                         'ai_model_shipped', 'ai_dataset_published',
                         'ai_agent_system_deployed', 'ai_paper_published',
                         'ai_benchmark_result', 'ai_safety_finding_validated',
                         'design_deliverable_validated',
                         'design_brand_system_delivered',
                         'design_typeface_released',
                         'design_system_adopted',
                         'design_contest_won',
                         'design_mission_delivered')
        OR cardinality(linked_deliverable_ids) >= 1
    );
