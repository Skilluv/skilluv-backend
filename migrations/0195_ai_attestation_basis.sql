-- What an AI attestation rests on.
--
-- The backlog asked for seven new `attestation_type` values. Migration 0178
-- already settled that argument for `code` and the reasoning holds here: the
-- three types — gesture, skill, compagnonnage — say what *kind* of thing is
-- attested, and each carries an invariant. "Published a dataset" is not a
-- kind; it is the evidence, and it can support a skill attestation or a
-- compagnonnage one. Adding it to `attestation_type` would mix two axes and
-- leave the invariants with no branch to attach to.
--
-- So: seven values on `basis`, the column that already exists for exactly
-- this. Nothing new is invented for AI.
--
-- ## Six of the seven must point at something
--
-- A model shipped, a dataset published, a paper written — each rests on an
-- artefact a stranger can open. The constraint requires the deliverable link
-- for all six, so the basis is a claim that can be checked rather than a
-- label. `featured_ai_researcher` is the exception: it is an editorial
-- decision about a person, not a claim about one artefact.

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
        -- AI. Each names something published somewhere a stranger can reach.
        'ai_model_shipped',
        'ai_dataset_published',
        'ai_agent_system_deployed',
        'ai_paper_published',
        'ai_benchmark_result',
        'ai_safety_finding_validated',
        'featured_ai_researcher'
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
                         'ai_benchmark_result', 'ai_safety_finding_validated')
        OR cardinality(linked_deliverable_ids) >= 1
    );
