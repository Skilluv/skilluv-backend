-- Per-channel notification preferences, and the locale a notification is
-- written in.
--
-- ─── What exists today ────────────────────────────────────────────
--
-- `NotificationService::send` writes a row, pushes over WebSocket and tries
-- a mobile push. Three channels, no way to decline any of them, and titles
-- passed in as hardcoded French strings by every caller.
--
-- Email preferences live apart in `user_email_preferences`, covering three
-- marketing-ish categories. It answers "may we send the weekly digest",
-- never "does this person want to know, and how".
--
-- ─── What this adds ───────────────────────────────────────────────
--
-- One row per (person, kind, channel), created only when someone changes
-- something. Absence means the default for that kind, so adding a new
-- notification does not require backfilling a row for every user — a table
-- of tens of millions of rows saying "yes, the default" is a cost with no
-- benefit.
--
-- Transactional mail is deliberately not represented. Password resets,
-- security alerts and payout receipts are not preferences: a person who
-- opted out of a payout receipt still needs to know their money moved.
-- `notification_kinds.transactional` marks those, and the resolver ignores
-- preferences for them.

-- Catalogue of what can be sent. A row here is what makes a kind
-- addressable: the resolver refuses an unknown kind rather than silently
-- sending on defaults nobody chose.
CREATE TABLE notification_kinds (
    -- Dotted, matching the i18n keys: `payout.sent` reads
    -- `notification.payout.sent.title` and `email.payout.sent.subject`.
    kind VARCHAR(60) PRIMARY KEY CHECK (kind ~ '^[a-z][a-z0-9_]*(\.[a-z0-9_]+)+$'),
    -- Grouping for the settings screen.
    category VARCHAR(30) NOT NULL,
    -- Channels this kind may use at all. A kind absent from here can never
    -- be sent on that channel, whatever the preference says: some things
    -- have no business being an email.
    allows_in_app BOOLEAN NOT NULL DEFAULT TRUE,
    allows_push BOOLEAN NOT NULL DEFAULT TRUE,
    allows_email BOOLEAN NOT NULL DEFAULT FALSE,
    -- Defaults applied when nobody has expressed a preference.
    default_in_app BOOLEAN NOT NULL DEFAULT TRUE,
    default_push BOOLEAN NOT NULL DEFAULT FALSE,
    default_email BOOLEAN NOT NULL DEFAULT FALSE,
    -- Cannot be declined. Reserved for what a person needs to know
    -- regardless: money moved, access changed, the law requires it.
    transactional BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A default cannot exceed what the kind allows, which would be a
    -- promise the sender cannot keep.
    CONSTRAINT notification_kinds_defaults_within_allowed CHECK (
        (NOT default_in_app OR allows_in_app)
        AND (NOT default_push OR allows_push)
        AND (NOT default_email OR allows_email)
    )
);

COMMENT ON TABLE notification_kinds IS
    'Every notification the platform can send, with which channels it may '
    'use and what happens when nobody chose. Kind names double as i18n keys.';

-- Only rows for people who changed something.
CREATE TABLE notification_preferences (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(60) NOT NULL REFERENCES notification_kinds(kind) ON DELETE CASCADE,
    channel VARCHAR(10) NOT NULL CHECK (channel IN ('in_app', 'push', 'email')),
    enabled BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, kind, channel)
);

CREATE INDEX idx_notification_preferences_user ON notification_preferences (user_id);

-- ─── Seed ─────────────────────────────────────────────────────────
--
-- Push defaults to off almost everywhere. A notification that buzzes a phone
-- is a claim on someone's attention, and defaulting it on is how an app
-- teaches people to disable notifications entirely.

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional) VALUES
    -- Money. All transactional: someone whose payout failed needs to know,
    -- whatever they ticked.
    ('payout.sent',         'payments', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  TRUE),
    ('payout.failed',       'payments', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  TRUE),
    ('payment.received',    'payments', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  TRUE),
    ('payment.refunded',    'payments', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  TRUE),
    ('funds.released',      'payments', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  TRUE),
    ('funds.disputed',      'payments', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  TRUE),
    -- Account and security. Transactional for the same reason.
    ('security.new_login',  'security', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  TRUE),
    ('security.password_changed', 'security', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, TRUE),
    ('account.capability_granted', 'account', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, TRUE),
    -- Learning. Declinable.
    ('challenge.reviewed',  'learning', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  FALSE),
    ('challenge.validated', 'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    ('attestation.issued',  'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    ('rank.promoted',       'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    ('badge.earned',        'learning', TRUE, TRUE, FALSE, TRUE, FALSE, FALSE, FALSE),
    -- Mentorship.
    ('mentorship.booked',   'mentorship', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    ('mentorship.reminder', 'mentorship', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    ('mentorship.cancelled','mentorship', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    -- Social. In-app only by default; this is the category that makes people
    -- mute an app.
    ('social.mention',      'social',   TRUE, TRUE, FALSE, TRUE, FALSE, FALSE, FALSE),
    ('social.reply',        'social',   TRUE, TRUE, FALSE, TRUE, FALSE, FALSE, FALSE),
    ('guild.invitation',    'social',   TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  FALSE),
    ('guild.application',   'social',   TRUE, TRUE, FALSE, TRUE, FALSE, FALSE, FALSE),
    -- Enterprise.
    ('enterprise.credits_low',   'enterprise', TRUE, FALSE, TRUE, TRUE, FALSE, TRUE, FALSE),
    ('enterprise.invoice_ready', 'enterprise', TRUE, FALSE, TRUE, TRUE, FALSE, TRUE, TRUE),
    ('enterprise.talent_replied','enterprise', TRUE, TRUE,  TRUE, TRUE, FALSE, TRUE, FALSE),
    -- Moderation and admin. Email on by default: these are queues someone
    -- has to work, and an in-app badge nobody sees is how a queue rots.
    ('admin.review_queued',      'admin', TRUE, FALSE, TRUE, TRUE, FALSE, TRUE, FALSE),
    ('admin.fraud_flagged',      'admin', TRUE, TRUE,  TRUE, TRUE, TRUE,  TRUE, FALSE),
    ('admin.payout_needs_replay','admin', TRUE, TRUE,  TRUE, TRUE, TRUE,  TRUE, TRUE),
    ('admin.reconciliation_drift','admin', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, TRUE);

CREATE OR REPLACE FUNCTION touch_notification_preferences_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_notification_preferences_updated_at
    BEFORE UPDATE ON notification_preferences
    FOR EACH ROW EXECUTE FUNCTION touch_notification_preferences_updated_at();

-- The locale a notification is written in, captured at send time.
--
-- Someone changing language should not rewrite the history of what they were
-- told, and a delivery that is retried has to be retried in the language it
-- was composed in.
ALTER TABLE notifications
    ADD COLUMN locale CHAR(2),
    -- The i18n key, kept alongside the rendered text. Lets the front
    -- re-render in a new language, and lets us fix a wording without
    -- migrating rows.
    ADD COLUMN kind VARCHAR(60),
    ADD COLUMN payload JSONB;

COMMENT ON COLUMN notifications.kind IS
    'notification_kinds.kind. NULL on rows predating migration 0155.';
COMMENT ON COLUMN notifications.locale IS
    'Locale the title and body were rendered in, at the time they were sent.';
