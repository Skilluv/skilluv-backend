-- SKI-296 (T3-02b) — moderation hold on a talent offer.
--
-- `active` alone cannot carry a moderation decision. It is the talent's own
-- pause switch: they flip it from `PATCH /api/talent-offers/{id}`, so a
-- moderator who merely set `active = FALSE` would be undone by the author's
-- next request. A gesture that the offending party can reverse is not a
-- moderation gesture.
--
-- So the hold lives in its own column. `active` keeps meaning "the author
-- wants this listed"; `moderation_held_at` means "the platform does not",
-- and the second wins. Lifting the hold is an admin action, never an
-- author one.
--
-- The row is never deleted: an offer under dispute has to stay readable
-- while the dispute is instructed.

ALTER TABLE talent_offers
    ADD COLUMN IF NOT EXISTS moderation_held_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS moderation_reason TEXT,
    ADD COLUMN IF NOT EXISTS moderated_by UUID REFERENCES users(id) ON DELETE SET NULL;

-- A hold always says why. An offer removed from the marketplace without a
-- recorded motive is the actual problem; the missing audit entry is only
-- its consequence.
ALTER TABLE talent_offers
    DROP CONSTRAINT IF EXISTS talent_offers_moderation_coherent;
ALTER TABLE talent_offers
    ADD CONSTRAINT talent_offers_moderation_coherent CHECK (
        (moderation_held_at IS NULL AND moderation_reason IS NULL)
        OR (moderation_held_at IS NOT NULL
            AND moderation_reason IS NOT NULL
            AND length(moderation_reason) >= 8)
    );

-- The admin queue reads "what is currently held" far more often than it
-- reads the whole table.
CREATE INDEX IF NOT EXISTS idx_talent_offers_moderation_held
    ON talent_offers (moderation_held_at DESC)
    WHERE moderation_held_at IS NOT NULL;
