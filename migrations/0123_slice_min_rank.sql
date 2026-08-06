-- SKI-78 (P26 v2 C-02) — minimum-rank restriction on project_slices.
--
-- A slice can now declare a `min_rank` (one of the P17 rank values). Only
-- users whose `user_ranks.rank` is at or above this level may claim. NULL
-- (the default) means no rank floor.
--
-- Rationale: some challenges (e.g. reviewing a critical migration, editing
-- a payment path) should not be claimed by first-day apprentis even if
-- the domain matches. This is a soft gate — admins can still override.

ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS min_rank VARCHAR(15)
    CHECK (min_rank IS NULL OR min_rank IN ('apprenti', 'ranger', 'artisan', 'maitre', 'doyen'));

CREATE INDEX IF NOT EXISTS idx_slices_min_rank
    ON project_slices (min_rank)
    WHERE min_rank IS NOT NULL;
