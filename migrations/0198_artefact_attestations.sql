-- A fourth kind of attestation: the one that rests on an artefact.
--
-- ## The collision
--
-- The three types were written for one story: somebody's skill level rises,
-- and the platform says so. `gesture` and `skill` therefore each name exactly
-- one skill node, which a CHECK enforces, and `compagnonnage` names a project.
--
-- Migration 0178 added `basis` for a different story: this person shipped a
-- library, or got a pull request merged, or contributed to a web standard.
-- None of those is a level-up in a named skill — a merged contribution to the
-- Linux kernel is not "C, level 4" — and filing them as `skill` meant either
-- inventing a skill node to point at or breaking the constraint.
--
-- So they get their own type. It links a deliverable rather than a skill,
-- which is what it is actually about.
--
-- ## Why not widen the skill constraint instead
--
-- Because the constraint is right. A `skill` attestation that named zero
-- skills would be unreadable, and the exemption would apply to every future
-- one as well. The narrower change is a new word for a new thing.

ALTER TABLE attestations
    DROP CONSTRAINT attestations_attestation_type_check;

ALTER TABLE attestations
    ADD CONSTRAINT attestations_attestation_type_check CHECK (
        attestation_type IN (
            'gesture',
            'skill',
            'compagnonnage',
            -- Rests on something public: a merged pull request, a published
            -- package, an accepted proposal. The `basis` column says which.
            'artefact'
        )
    );

COMMENT ON COLUMN attestations.attestation_type IS
    'What kind of statement this is. gesture/skill follow a level-up and name '
    'a skill; compagnonnage names a project; artefact rests on something '
    'public and names the deliverable, with `basis` saying what kind.';

-- An artefact attestation with no basis says nothing about what it rests on,
-- which is the only thing that distinguishes it from a note.
ALTER TABLE attestations
    ADD CONSTRAINT attestations_artefact_states_its_basis CHECK (
        attestation_type <> 'artefact' OR basis IS NOT NULL
    );

-- The same artefact must not be attested twice: two rows for one merged pull
-- request would double every count that reads them, including the craft score.
CREATE UNIQUE INDEX uniq_attestations_artefact_per_deliverable
    ON attestations (user_id, basis, linked_deliverable_ids)
    WHERE attestation_type = 'artefact'
      AND revoked_at IS NULL
      AND cardinality(linked_deliverable_ids) > 0;
