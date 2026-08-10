-- SKI-34 (Hygiène pré-prod HYG-07) — Discord notifications queue.
--
-- Queue table for events that the Discord bot picks up and posts to
-- the Skilluv Discord channels. Producer (backend event hooks) inserts;
-- consumer (skilluv-discord-notifier binary) polls, posts, marks sent.
--
-- Event types (payload_json shape depends on type):
--   * "rank_promotion" — user promoted → post in #promotions
--   * "badge_earned"   — user earned a badge → post in #promotions
--   * "attestation_new" — new attestation minted → post in #annonces
--   * "slice_validated" — challenge validated → post in #annonces
--
-- Consumer marks `sent_at` after successful Discord POST. Failed sends
-- keep `sent_at = NULL` and are retried on the next poll (via `failed_count`
-- backoff — capped so a dead channel doesn't burn attempts forever).

CREATE TABLE IF NOT EXISTS discord_notifications_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL CHECK (event_type IN (
        'rank_promotion',
        'badge_earned',
        'attestation_new',
        'slice_validated'
    )),
    payload_json JSONB NOT NULL,
    -- Target channel id (Discord). NULL → default channel from env.
    target_channel_id TEXT,
    sent_at TIMESTAMPTZ,
    failed_count SMALLINT NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Consumer polls "pending, not exhausted".
CREATE INDEX IF NOT EXISTS idx_discord_notifications_pending
    ON discord_notifications_queue (created_at ASC)
    WHERE sent_at IS NULL AND failed_count < 10;
