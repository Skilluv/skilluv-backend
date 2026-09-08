-- The footer newsletter form stops throwing addresses away (SKI-369).
--
-- What it did: waited 400ms so the button looked busy, cleared the field so it
-- looked accepted, and discarded the address. Every signal the interface gave
-- said the person had subscribed. They had not, and had no way to find out.
-- That is worse than having no form.
--
-- Keyed by email, not by user, because `notification_preferences` is keyed by
-- `user_id REFERENCES users(id)`: a preference row cannot exist without an
-- account. Putting the footer form there would mean minting a user for
-- somebody who gave an address and nothing else.
--
-- (`user_email_preferences` and its `marketing` boolean are what the frontend
-- and I both reached for first. Migration 0164 dropped that table: it was a
-- second preference system that disagreed with the catalogue, and the
-- catalogue won. The nearest equivalent is the `lifecycle` category,
-- described there as "off by default in every channel and opted into
-- explicitly, because that is what marketing consent is" - the newsletter
-- borrows that stance and not that category, for the reason set out beside
-- the INSERT below.)
--
-- So the newsletter is registered below as a kind in that catalogue rather
-- than bolted beside it, and the two records are joined at send time by a
-- rule: **any opt-out wins**. An address is mailed only if this row is
-- confirmed and, where an account exists with the same address, that account
-- has not turned the newsletter off. Unsubscribing anywhere is then effective
-- everywhere. The alternative, one of them being authoritative, means
-- somebody who clicks unsubscribe in a mail keeps receiving it because they
-- ticked a box in their settings two years ago.
--
-- Double opt-in, not single: an address is only mailed after the person
-- holding it clicks the link. Anyone can type somebody else's address into a
-- footer form.

CREATE TABLE newsletter_subscriptions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Stored lowercased and unique, which is what makes re-subscription an
    -- ordinary update rather than a second row nobody can unsubscribe from.
    email        TEXT NOT NULL UNIQUE
                 CHECK (email = lower(email) AND email LIKE '%_@_%.__%'),

    -- 'pending'      an address given, confirmation sent, nothing may be mailed
    -- 'confirmed'    the person clicked the link
    -- 'unsubscribed' they asked to stop; the row stays so a later re-subscribe
    --                is an update and so the record of consent survives
    status       VARCHAR(16) NOT NULL DEFAULT 'pending'
                 CHECK (status IN ('pending', 'confirmed', 'unsubscribed')),

    locale       VARCHAR(5) NOT NULL DEFAULT 'fr'
                 CHECK (locale IN ('fr', 'en', 'ar')),

    -- Where the address came from, so a list can be explained later.
    source       VARCHAR(40) NOT NULL DEFAULT 'footer',

    -- Consent, as a record rather than a boolean.
    --
    -- The wording shown beside the field is stored, not just the fact that
    -- something was agreed to. If the wording changes, a consent gathered
    -- under the old one has to stay readable as that consent.
    consent_text TEXT,
    consent_ip   VARCHAR(45),
    consent_user_agent TEXT,

    -- The tokens. Both are opaque and single-purpose.
    --
    -- `unsubscribe_token` never expires and needs no account: the person who
    -- wants out is often the person who never had one, and asking them to
    -- sign in to stop receiving mail is how a list becomes spam.
    confirm_token        TEXT UNIQUE,
    confirm_token_expires_at TIMESTAMPTZ,
    unsubscribe_token    TEXT NOT NULL UNIQUE,

    confirmed_at    TIMESTAMPTZ,
    unsubscribed_at TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A confirmed row has a date; a pending one does not. Without this the
    -- two can disagree and nothing notices.
    CONSTRAINT newsletter_confirmed_has_a_date CHECK (
        (status = 'confirmed') = (confirmed_at IS NOT NULL)
    ),
    CONSTRAINT newsletter_unsubscribed_has_a_date CHECK (
        (status = 'unsubscribed') = (unsubscribed_at IS NOT NULL)
    )
);

COMMENT ON TABLE newsletter_subscriptions IS
    'Newsletter addresses, keyed by email so somebody with no account can subscribe. Joined to user_email_preferences.marketing at send time by: any opt-out wins.';

-- The send query filters on status; the confirm and unsubscribe endpoints
-- look up by token. Both tokens are already UNIQUE, which indexes them.
CREATE INDEX idx_newsletter_status ON newsletter_subscriptions (status)
    WHERE status = 'confirmed';

-- Its own touch function, following the convention the rest of the schema
-- uses (touch_missions_updated_at and eleven siblings). There is no shared
-- one to reuse.
CREATE OR REPLACE FUNCTION touch_newsletter_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_newsletter_subscriptions_updated_at
    BEFORE UPDATE ON newsletter_subscriptions
    FOR EACH ROW EXECUTE FUNCTION touch_newsletter_updated_at();

-- The newsletter, as a kind the settings screen already knows how to render.
--
-- Its own category, and not `lifecycle`, which is where it nearly went.
--
-- `GET/PUT /users/me/email-preferences` is a narrower view over this
-- catalogue: its `marketing` boolean reads true when ANY `lifecycle` kind has
-- email enabled, and writing it false writes false across every one of them
-- (routes/email_prefs.rs, `lifecycle_kinds`). A newsletter filed under
-- `lifecycle` would therefore be switched off by somebody using a coarse
-- toggle to stop the onboarding drip - an explicit, confirmed, double opt-in
-- consent silently overridden by a control that was never about it, with the
-- subscription row still reading `confirmed` and nothing to show the person
-- why they stopped receiving anything.
--
-- Two records disagreeing about whether somebody may be mailed is the exact
-- failure this table was shaped to avoid, so the newsletter answers only to
-- its own toggle and its own unsubscribe link.
--
-- Off in every channel by default and opted into explicitly. `transactional`
-- is FALSE: nothing here is owed to anybody, and a transactional kind cannot
-- be refused.
INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional)
VALUES
    ('newsletter.issue', 'newsletter', FALSE, FALSE, TRUE,
     FALSE, FALSE, FALSE, FALSE)
ON CONFLICT (kind) DO NOTHING;
