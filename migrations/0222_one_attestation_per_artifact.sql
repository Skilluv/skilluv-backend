-- An artefact attestation is one per artefact, not one per skill.
--
-- ## The index that would have blocked the generators
--
-- `uniq_attestations_gesture_skill_per_skill` says a person gets at most one
-- `skill` attestation per skill node. That is right for what it was written
-- for in 0068: the level-up generator, which attests reaching level four in a
-- skill, and reaching it twice is not a thing.
--
-- Migration 0178 introduced a second kind of attestation on the same type — a
-- claim resting on an artefact, carrying a `basis`. Somebody who ships two
-- models both resting on `deep-learning-training` has done two things, and
-- the old index would have silently swallowed the second: `ON CONFLICT DO
-- NOTHING` and no attestation, or a unique violation surfacing as a 500.
--
-- So the old index keeps its job and narrows to it, and a new one states the
-- rule that actually applies to artefact attestations: one per person, per
-- basis, per artefact. Re-running a generator over work already attested does
-- nothing, which is what makes it safe to run from a hook.

DROP INDEX IF EXISTS uniq_attestations_gesture_skill_per_skill;

CREATE UNIQUE INDEX uniq_attestations_gesture_skill_per_skill
    ON attestations (user_id, attestation_type, linked_skill_node_ids)
    WHERE attestation_type IN ('gesture', 'skill')
      AND basis IS NULL
      AND revoked_at IS NULL;

COMMENT ON INDEX uniq_attestations_gesture_skill_per_skill IS
    'One level-up attestation per skill. Only for those: an attestation that '
    'rests on an artefact is bounded by the artefact, not by the skill it '
    'happens to name.';

CREATE UNIQUE INDEX uniq_attestations_per_artifact_basis
    ON attestations (user_id, basis, linked_deliverable_ids)
    WHERE basis IS NOT NULL AND revoked_at IS NULL;

COMMENT ON INDEX uniq_attestations_per_artifact_basis IS
    'One attestation per person, per basis, per set of deliverables. Makes a '
    'generator safe to re-run: the second pass over already-attested work '
    'inserts nothing instead of duplicating the claim.';
