-- The queue that carries "this person's Discord roles are out of date".
--
-- ## Why a queue and not a direct call
--
-- Only one process holds `DISCORD_BOT_TOKEN`: the bot. That is deliberate —
-- the token can create channels, delete them and grant any role, and the HTTP
-- backend has no business holding a credential that powerful. So the backend
-- decides *what* should be true and the bot makes it true, the same split the
-- notification queue already uses.
--
-- ## Why one pending row per person, not one per event
--
-- A single validated deliverable can move a rank, grant a capability and fire
-- three proof hooks. Queueing each would have the bot compute the same answer
-- four times and issue the same Discord writes four times, against an API that
-- rate-limits per route. The partial unique index below collapses them: while
-- a sync is pending for somebody, further requests update the reason instead
-- of adding a row.
--
-- ## Why `reason` is kept
--
-- Roles appear and disappear on a server other people watch. When somebody
-- asks why they lost `@Relecteur`, the answer has to be findable, and
-- "capability_revoked" beside a timestamp is that answer.

CREATE TABLE discord_role_sync_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- What asked for it: 'linked', 'unlinked', 'rank_changed',
    -- 'capabilities_changed', 'orientations_changed', 'sweep'.
    reason TEXT NOT NULL,

    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_at TIMESTAMPTZ,

    -- What the worker actually did, for the question above.
    roles_added TEXT[] NOT NULL DEFAULT '{}',
    roles_removed TEXT[] NOT NULL DEFAULT '{}',

    failed_count SMALLINT NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
    last_error TEXT
);

-- One pending row per person. `applied_at IS NULL` makes it partial, so the
-- history of applied syncs is kept and does not fight the constraint.
CREATE UNIQUE INDEX discord_role_sync_one_pending_per_user
    ON discord_role_sync_queue (user_id)
    WHERE applied_at IS NULL;

-- What the worker polls. Mirrors the notification queue's cut-off: ten
-- failures and the row stops being retried, because a row that has failed ten
-- times is a bug to read, not a transient error to keep hammering.
CREATE INDEX idx_discord_role_sync_pending
    ON discord_role_sync_queue (requested_at)
    WHERE applied_at IS NULL AND failed_count < 10;

CREATE INDEX idx_discord_role_sync_user_history
    ON discord_role_sync_queue (user_id, requested_at DESC);

COMMENT ON TABLE discord_role_sync_queue IS
    'Requests for the Discord bot to reconcile one member''s roles against the '
    'platform. Written by the backend, drained by skilluv-discord-bot, which is '
    'the only process holding the bot token.';
