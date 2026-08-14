-- Somewhere to put a message whose channel just failed.
--
-- ─── What happens today ───────────────────────────────────────────
--
-- An email is built, handed to Brevo, and if Brevo answers 503 the error is
-- logged and the message is gone. Not late — gone. There is nowhere to put
-- it, so there is nothing to retry, and the recipient's only clue is that
-- something they expected never arrived.
--
-- A push is worse: its failure is logged at debug level and swallowed, on
-- the reasonable grounds that a device token goes stale every time someone
-- reinstalls. Reasonable for a mention. Not reasonable for a payout that
-- failed, where the push was the fast path and nothing took its place.
--
-- ─── Two jobs, one table ──────────────────────────────────────────
--
-- **Retry.** A channel that failed for a transient reason is tried again,
-- with a backoff, until it works or until it has clearly stopped being
-- transient — at which point a person is told rather than the message
-- being dropped quietly.
--
-- **Fallback.** A push that fails on a transactional kind enqueues an
-- email instead. Not a retry of the push: a stale token does not heal, and
-- asking again is asking the same question. The point is to reach the
-- person by another road, and only where it matters — nobody needs an
-- email because a push about a mention did not arrive.
--
-- ─── Why the rendered text is not stored ──────────────────────────
--
-- The row keeps what the message *is* — kind, locale, title, body,
-- payload — and the worker renders it again. Storing the HTML would mean a
-- template fix never reaches a queued message, and a queue holding a
-- hundred thousand copies of the same frame.

CREATE TABLE notification_outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- The durable notification this belongs to, where there is one. NULL
    -- for a channel that failed before the row was written.
    notification_id UUID REFERENCES notifications(id) ON DELETE SET NULL,

    kind VARCHAR(60) NOT NULL REFERENCES notification_kinds(kind) ON DELETE CASCADE,
    channel VARCHAR(10) NOT NULL CHECK (channel IN ('push', 'email')),

    -- Already translated and interpolated, in the recipient's language. The
    -- worker re-renders the frame around them, not the words: re-resolving
    -- the copy later would silently change a message someone was already
    -- told about through another channel.
    locale VARCHAR(10) NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    payload JSONB,
    cta_url TEXT,
    unsubscribe_url TEXT,

    -- TRUE when this exists because another channel failed, not because the
    -- recipient asked for it. Such a row ignores the preference for its
    -- channel — which is only ever done for transactional kinds, and is
    -- recorded here so that is auditable rather than implicit.
    is_fallback BOOLEAN NOT NULL DEFAULT FALSE,

    status VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (status IN (
            'pending',    -- waiting for its next attempt
            'sent',       -- the channel accepted it
            'abandoned'   -- out of attempts; a human was told
        )),

    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    -- Exponential backoff, computed on each failure. A row is invisible to
    -- the worker until this passes.
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- The provider's own words from the last failure. "Email failed" is not
    -- something anyone can act on; "550 mailbox unavailable" is.
    last_error TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A row that is done has nothing left to attempt, and one that is
    -- pending has not been sent. Enforced rather than assumed, because the
    -- worker's query trusts it.
    CONSTRAINT outbox_abandoned_is_explained CHECK (
        status <> 'abandoned' OR last_error IS NOT NULL
    )
);

-- The worker's only query: what is due, oldest first.
CREATE INDEX idx_notification_outbox_due
    ON notification_outbox (next_attempt_at)
    WHERE status = 'pending';

CREATE INDEX idx_notification_outbox_user
    ON notification_outbox (user_id, created_at DESC);

COMMENT ON TABLE notification_outbox IS
    'Channels that failed and are worth trying again, and fallbacks written '
    'when a transactional push could not be delivered. Drained by '
    'services::outbox.';

CREATE OR REPLACE FUNCTION touch_notification_outbox_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_notification_outbox_updated_at
    BEFORE UPDATE ON notification_outbox
    FOR EACH ROW EXECUTE FUNCTION touch_notification_outbox_updated_at();
