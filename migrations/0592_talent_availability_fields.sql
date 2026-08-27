-- Talent availability: a rate range and a next-available date.
--
-- The design profile already reads users.available_for_hire and looking_for —
-- a general "open to work" declaration. What SKI-253's availability section
-- needs, and what the front could not build (SKI-315), is the mission-specific
-- pair: a day-rate range and the date the person is next free. Added to users
-- rather than a design-only table because the same section is wanted for cyber
-- and any mission-marketplace domain — one place for all of them.
--
-- Product decisions taken here, stated so they are not silently assumed:
--   * The rate is a RANGE, never an exact price — a price is negotiated, a
--     range is filtered on — and it is public: a recruiter filtering on it
--     wastes fewer people's time, which is the point of showing it.
--   * Freshness rides on users.updated_at (the availability PUT touches it) and
--     on available_from itself; there is no hard auto-expiry, so a client greys
--     a stale badge rather than the server deleting a true-yesterday fact.

ALTER TABLE users
    ADD COLUMN day_rate_range TEXT,
    ADD COLUMN available_from DATE;

COMMENT ON COLUMN users.day_rate_range IS
    'A day-rate range a talent is open to for missions, e.g. "300-500 EUR". A '
    'range, not a price, and public. NULL = not stated. Paired with '
    'available_for_hire (the badge) and available_from (SKI-315).';
COMMENT ON COLUMN users.available_from IS
    'The date the talent is next free for a mission. NULL = not stated.';
