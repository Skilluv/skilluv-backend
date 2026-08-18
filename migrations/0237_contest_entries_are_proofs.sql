-- Winning a contest has to leave a proof, like everything else does.
--
-- ## The gap
--
-- `deliverables` accepts two parents: a slice or a challenge. A contest entry
-- is neither, so nothing a contest produces can be written to the proof
-- table. Today somebody wins a hackathon and their rank does not move, their
-- badges do not fire, their portfolio shows nothing, and recruiter search
-- cannot see it. The prize fragments land and that is the whole of it.
--
-- That is not a design problem — it has been true of code contests since
-- migration 0189 — but design is where it becomes untenable, because a brief
-- contest is one of the two ways a designer is meant to prove anything here.
--
-- ## Why a third parent and not a synthetic slice
--
-- The alternative was to mint a `project_slices` row per winning entry so the
-- existing parent would fit. That would put a claimable, browsable unit of
-- work in the backlog for something already finished, and every listing query
-- would then need to exclude it. A nullable column and one more branch in a
-- CHECK is smaller and says what is actually true: this proof came from a
-- contest.
--
-- ## Which entries earn one
--
-- Not every entry. A deliverable is a verified artefact, and taking part is
-- not an achievement — the platform's whole premise is that the proof means
-- something. The service writes one for podium finishers only; the schema
-- does not enforce that, because "podium" is a ranking decision and rankings
-- are recomputed.

ALTER TABLE deliverables
    ADD COLUMN tournament_submission_id UUID
        REFERENCES tournament_submissions(id) ON DELETE SET NULL;

COMMENT ON COLUMN deliverables.tournament_submission_id IS
    'The contest entry this proof came from. A third parent alongside '
    'slice_id and challenge_id: a contest entry is neither, and without this '
    'winning a contest moved nothing on a profile.';

ALTER TABLE deliverables DROP CONSTRAINT IF EXISTS deliverables_at_least_one_parent;

ALTER TABLE deliverables
    ADD CONSTRAINT deliverables_at_least_one_parent
    CHECK (
        slice_id IS NOT NULL
        OR challenge_id IS NOT NULL
        OR tournament_submission_id IS NOT NULL
    );

-- One proof per entry. A recomputed ranking must not pay a second time.
CREATE UNIQUE INDEX uniq_deliverables_per_contest_entry
    ON deliverables (tournament_submission_id)
    WHERE tournament_submission_id IS NOT NULL;

-- "What did this contest produce" — read by the public contest report and by
-- the attestation generators.
CREATE INDEX idx_deliverables_by_contest_entry
    ON deliverables (tournament_submission_id)
    WHERE tournament_submission_id IS NOT NULL;
