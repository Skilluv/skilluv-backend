-- A paid mission produces a deliverable, and the table had nowhere to put it.
--
-- ## What refused it
--
-- `deliverables_at_least_one_parent` requires a slice, a challenge or a
-- tournament submission. A mission is none of the three, so a mission's
-- artefact could not be recorded at all.
--
-- That mattered the moment `design_mission_delivered` was wired up: migration
-- 0233 rules that every artefact basis must link a deliverable, so the
-- attestation for an accepted mission was refused by the database — correctly.
-- The constraint was right and the model was short of a parent.
--
-- ## Why the delivery and not the mission
--
-- A mission is handed in more than once: `mission_deliveries` carries the
-- rounds, and only the accepted round is the artefact. Pointing at the mission
-- would leave "which of the three versions is this attestation about" with no
-- answer, which is the question an attestation exists to settle.

ALTER TABLE deliverables
    ADD COLUMN IF NOT EXISTS mission_delivery_id UUID
        REFERENCES mission_deliveries(id) ON DELETE CASCADE;

COMMENT ON COLUMN deliverables.mission_delivery_id IS
    'The accepted round of a paid mission. One of the four parents a '
    'deliverable can have, and the only one that was paid for.';

ALTER TABLE deliverables
    DROP CONSTRAINT IF EXISTS deliverables_at_least_one_parent;

ALTER TABLE deliverables
    ADD CONSTRAINT deliverables_at_least_one_parent
    CHECK (
        slice_id IS NOT NULL
        OR challenge_id IS NOT NULL
        OR tournament_submission_id IS NOT NULL
        OR mission_delivery_id IS NOT NULL
    );

-- One deliverable per accepted round. Accepting twice is already impossible —
-- `mission_deliveries.decision` is written once — but the index says so here
-- too, because a retried attestation must not leave a second artefact behind.
CREATE UNIQUE INDEX IF NOT EXISTS uniq_deliverable_per_mission_delivery
    ON deliverables (mission_delivery_id)
    WHERE mission_delivery_id IS NOT NULL;
