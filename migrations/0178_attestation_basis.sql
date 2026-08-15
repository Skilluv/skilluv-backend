-- What an attestation rests on.
--
-- ## Why not seven more attestation types
--
-- `attestation_type` has three values — gesture, skill, compagnonnage — and
-- each carries an invariant: a compagnonnage attestation must link at least
-- one project, a gesture or skill exactly one skill node. They describe the
-- *kind* of thing being attested.
--
-- "PR merged upstream" and "library published" are not kinds. They are what
-- the attestation rests on, and both can be the basis of a skill attestation
-- or of a compagnonnage one. Folding them into the same column would mix two
-- axes and leave the invariants with no branch to attach to — an attestation
-- of type `code_rfc_accepted` links neither a skill node nor a project by the
-- existing rules, so every one of them would be refused or the rules would
-- have to be dropped.
--
-- A second column says the second thing. The two stay orthogonal, and the
-- existing invariants keep meaning what they meant.
--
-- ## Why the basis is nullable
--
-- Every attestation issued before this column existed rests on something,
-- but nobody recorded what. Backfilling a guess would put a claim in the
-- record that no human made. NULL says "not stated", which is true.

ALTER TABLE attestations
    ADD COLUMN basis VARCHAR(40);

COMMENT ON COLUMN attestations.basis IS
    'The evidence this attestation rests on — a merged pull request, a '
    'published library, an accepted RFC. Orthogonal to attestation_type, '
    'which says what kind of thing is being attested. NULL on attestations '
    'issued before the column existed: not stated, rather than guessed.';

ALTER TABLE attestations
    ADD CONSTRAINT attestations_basis_check
    CHECK (basis IS NULL OR basis IN (
        -- Code. Each names something with an artefact behind it, because an
        -- attestation whose basis cannot be pointed at is an opinion.
        'code_pr_merged_upstream',
        'code_project_shipped',
        'code_library_published',
        'code_rfc_accepted',
        'code_standard_contribution',
        'code_devtool_adopted',
        'featured_coder'
    ));

CREATE INDEX idx_attestations_basis
    ON attestations (basis)
    WHERE basis IS NOT NULL AND revoked_at IS NULL;

-- An attestation resting on a merged pull request has to link the deliverable
-- that carries it. Otherwise the basis is a label rather than a claim anyone
-- can check — and the whole point of these is that a stranger can.
ALTER TABLE attestations
    ADD CONSTRAINT attestations_artifact_basis_links_a_deliverable
    CHECK (
        basis IS NULL
        OR basis NOT IN ('code_pr_merged_upstream', 'code_project_shipped',
                         'code_library_published')
        OR cardinality(linked_deliverable_ids) >= 1
    );
