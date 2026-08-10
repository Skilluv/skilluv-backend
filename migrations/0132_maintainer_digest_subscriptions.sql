-- SKI-120 (P26 v2 J-02) — maintainer weekly digest opt-in.
--
-- External-repo maintainers can subscribe to receive a weekly summary of
-- Skilluv activity on their repos: how many contributors touched slices,
-- which PRs got validated. Zero spam — double opt-in (subscribe →
-- confirmation email → click confirms), unsubscribe token in every email.
--
-- We DO NOT auto-subscribe maintainers even when we detect shadow-
-- contributions on their repos. Community-first policy: they come to us,
-- we don't mail-blast.

CREATE TABLE IF NOT EXISTS maintainer_digest_subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The maintainer's GitHub login (for our records, not verified).
    github_login TEXT NOT NULL CHECK (length(github_login) BETWEEN 1 AND 80),
    email TEXT NOT NULL CHECK (email ~ '^.+@.+\..+$'),
    -- Repos they want digest for, e.g. ['launchbadge/sqlx', 'launchbadge/sqlxmigrator'].
    repos TEXT[] NOT NULL CHECK (array_length(repos, 1) BETWEEN 1 AND 50),
    -- Random URL-safe token for confirm + unsubscribe links.
    confirm_token TEXT NOT NULL UNIQUE,
    unsubscribe_token TEXT NOT NULL UNIQUE,
    confirmed_at TIMESTAMPTZ,
    unsubscribed_at TIMESTAMPTZ,
    last_digest_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Prevent multiple pending signups for the same (email, repo set) —
-- second POST just re-mails the confirmation, no new row.
CREATE UNIQUE INDEX IF NOT EXISTS uq_maintainer_digest_email_active
    ON maintainer_digest_subscriptions (email)
    WHERE unsubscribed_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_maintainer_digest_confirmed
    ON maintainer_digest_subscriptions (confirmed_at)
    WHERE unsubscribed_at IS NULL AND confirmed_at IS NOT NULL;
