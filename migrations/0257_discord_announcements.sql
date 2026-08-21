-- The Discord queue nobody was writing to.
--
-- ## What was actually there
--
-- Migration 0135 built `discord_notifications_queue` with four event types and
-- a consumer that polls it. It has no producer. Not a trigger, not a call site
-- in the backend — searching the whole tree for the table name finds the two
-- bot binaries and nothing else. The queue has never carried a row, which is
-- why nobody noticed that the consumer ignores `target_channel_id` and routes
-- on a hardcoded pair of channels instead.
--
-- Adding five more event types to that would have been five more events
-- nobody enqueues.
--
-- ## Announcing to a room is not notifying a person
--
-- This stays out of `notify`, which delivers to a person across three channels
-- they can each switch off. A Discord post is an announcement in a public
-- room: there is no recipient to hold a preference, and giving it one would
-- mean a toggle that silences a channel for everybody who ever touched it.
--
-- ## Channels are configuration
--
-- The consumer hardcoded `#promotions` and `#annonces` from two environment
-- variables. Seven domains times six kinds of room is forty-two variables and
-- a redeploy every time a channel is renamed — so a table, and a queue row
-- carries the channel it resolved to at enqueue time.
--
-- Resolved at enqueue rather than at post: which room an announcement belonged
-- in is a fact about the moment it happened, and re-resolving at post time
-- would silently move a week-old backlog into a channel that has since been
-- repurposed.

ALTER TABLE discord_notifications_queue
    DROP CONSTRAINT IF EXISTS discord_notifications_queue_event_type_check;

ALTER TABLE discord_notifications_queue
    ADD CONSTRAINT discord_notifications_queue_event_type_check
    CHECK (event_type IN (
        -- Migration 0135.
        'rank_promotion',
        'badge_earned',
        'attestation_new',
        'slice_validated',
        -- A contest opened for entries.
        'contest_opened',
        -- A contest concluded and somebody won it.
        'contest_won',
        -- The week's editorial featuring.
        'talent_featured',
        -- An enterprise published a paid mission.
        'mission_posted'
    ));

-- Which domain the announcement is about, so it can be routed without the
-- consumer parsing the payload. Nullable: a rank promotion belongs to a
-- person, not to a domain.
--
-- No foreign key. The domain list is `validators::SKILL_DOMAINS` and the
-- CHECKs that quote it, not a table — `skill_domains` exists on older
-- databases and not in the canonical chain, so a reference to it would
-- migrate on a developer's machine and fail on a fresh one. Which is exactly
-- what it did.
ALTER TABLE discord_notifications_queue
    ADD COLUMN IF NOT EXISTS skill_domain VARCHAR(20);

COMMENT ON COLUMN discord_notifications_queue.target_channel_id IS
    'The room this was routed to, resolved at enqueue time from '
    '`discord_channels`. Null means the consumer falls back to its default '
    'channel — which is what happens when a purpose has no room configured, '
    'and is better than dropping the announcement.';

-- ═══════════════════════════════════════════════════════════════════
-- Where each kind of announcement goes
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS discord_channels (
    -- What the room is for: `contests`, `winners`, `showcase`, `missions`,
    -- `general`, `promotions`. Not the room's name — a name is Discord's to
    -- change, and the routing must survive a rename.
    purpose VARCHAR(30) NOT NULL,

    -- Which domain's room. Empty string rather than NULL for the
    -- domain-blind room, so the primary key stays simple and a lookup never
    -- has to reason about NULL equality.
    skill_domain VARCHAR(20) NOT NULL DEFAULT '',

    -- The Discord snowflake. Text, not a number: a snowflake exceeds what a
    -- signed 64-bit column can hold in the general case, and nothing here
    -- does arithmetic on it.
    channel_id TEXT NOT NULL CHECK (channel_id ~ '^[0-9]{5,25}$'),

    -- What it is called today, for the admin who has to recognise it. Never
    -- read by the router.
    label VARCHAR(80),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (purpose, skill_domain)
);

COMMENT ON TABLE discord_channels IS
    'Which Discord room each kind of announcement goes to. A row per '
    '(purpose, domain); the row with an empty domain is the fallback, so a '
    'server that has one #contests channel for everybody configures one row '
    'and a server that splits them per domain configures seven.';

-- No seed. Every value here is a snowflake from a specific Discord server,
-- and inventing one would route real announcements into a room that does not
-- exist — the consumer would then burn its ten retries against it.
