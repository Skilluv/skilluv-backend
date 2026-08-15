-- Lifecycle email joins the catalogue, and the second preference table goes.
--
-- ─── Two systems ──────────────────────────────────────────────────
--
-- `notification_preferences` answers "does this person want to hear about
-- X, and how" for every kind in the catalogue. `user_email_preferences`
-- answered the same question, worse, for three hardcoded categories —
-- `digest_weekly`, `streak_reminder`, `marketing` — and the digest and drip
-- services read only that one.
--
-- So a person who turned every email off in the settings screen still got
-- the weekly digest and the onboarding sequence, because those read a
-- different table. That is not a preference system, it is two of them
-- disagreeing.
--
-- The catalogue wins: it is per-kind rather than per-category, it carries
-- the channel split, and it is what the settings screen already renders.
--
-- ─── A toggle that did nothing ────────────────────────────────────
--
-- `streak_reminder` has been in that table since phase 1.7 and nothing has
-- ever sent a streak reminder. It is registered here as a real kind so the
-- toggle means something, and the sender lands with it.

-- ─── The kinds that were sending email outside the catalogue ──────

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional) VALUES

    -- The weekly digest. No in-app copy by default: a summary of a week
    -- the person lived through is not news, and putting it in the bell
    -- makes the bell worth ignoring.
    ('digest.weekly',      'digest', TRUE, TRUE, TRUE, FALSE, FALSE, TRUE, FALSE),

    -- The streak reminder the old table promised. Push rather than email:
    -- a nudge that arrives tomorrow is not a nudge.
    ('streak.reminder',    'digest', TRUE, TRUE, TRUE, FALSE, TRUE, FALSE, FALSE),

    -- Onboarding and retention. Off by default in every channel and opted
    -- into explicitly, because that is what marketing consent is.
    ('lifecycle.activate',    'lifecycle', TRUE, TRUE, TRUE, FALSE, FALSE, FALSE, FALSE),
    ('lifecycle.join_guild',  'lifecycle', TRUE, TRUE, TRUE, FALSE, FALSE, FALSE, FALSE),
    ('lifecycle.silent',      'lifecycle', TRUE, TRUE, TRUE, FALSE, FALSE, FALSE, FALSE),
    ('lifecycle.last_chance', 'lifecycle', TRUE, TRUE, TRUE, FALSE, FALSE, FALSE, FALSE),
    ('lifecycle.enterprise_welcome', 'lifecycle', TRUE, TRUE, TRUE, FALSE, FALSE, FALSE, FALSE),
    ('lifecycle.enterprise_demo',    'lifecycle', TRUE, TRUE, TRUE, FALSE, FALSE, FALSE, FALSE),
    ('lifecycle.enterprise_value',   'lifecycle', TRUE, TRUE, TRUE, FALSE, FALSE, FALSE, FALSE);

-- ─── Carry the answers people already gave ────────────────────────
--
-- Only the explicit ones. A row storing the default is indistinguishable
-- from never having been asked, and writing those would turn "I never
-- touched this" into "I decided this" for every account.

-- Someone who turned the digest off stays off.
INSERT INTO notification_preferences (user_id, kind, channel, enabled)
SELECT user_id, 'digest.weekly', 'email', FALSE
  FROM user_email_preferences
 WHERE digest_weekly = FALSE
ON CONFLICT (user_id, kind, channel) DO NOTHING;

-- Someone who turned streak reminders off never receives the ones this
-- migration makes possible.
INSERT INTO notification_preferences (user_id, kind, channel, enabled)
SELECT user_id, 'streak.reminder', channel.name, FALSE
  FROM user_email_preferences,
       (VALUES ('push'), ('email')) AS channel(name)
 WHERE streak_reminder = FALSE
ON CONFLICT (user_id, kind, channel) DO NOTHING;

-- Marketing consent was one box for six sequences. Someone who ticked it
-- consented to all of them, so it is carried to all of them; the default
-- is off, so only the yes matters.
INSERT INTO notification_preferences (user_id, kind, channel, enabled)
SELECT p.user_id, k.kind, 'email', TRUE
  FROM user_email_preferences p
 CROSS JOIN (
     SELECT kind FROM notification_kinds WHERE category = 'lifecycle'
 ) k
 WHERE p.marketing = TRUE
ON CONFLICT (user_id, kind, channel) DO NOTHING;

DROP TABLE user_email_preferences;

-- ─── The sequences keep their memory ──────────────────────────────
--
-- `email_log.kind` is what stops a sequence sending twice, and the drip
-- service now names its messages after the catalogue. Without this, every
-- account that already received the day-one email would receive it again.

UPDATE email_log SET kind = 'lifecycle.activate'    WHERE kind = 'drip_talent_d1_activate';
UPDATE email_log SET kind = 'lifecycle.join_guild'  WHERE kind = 'drip_talent_d3_join_guild';
UPDATE email_log SET kind = 'lifecycle.silent'      WHERE kind = 'drip_talent_d14_silent';
UPDATE email_log SET kind = 'lifecycle.last_chance' WHERE kind = 'drip_talent_d30_last_chance';
UPDATE email_log SET kind = 'lifecycle.enterprise_welcome' WHERE kind = 'drip_ent_d1_welcome';
UPDATE email_log SET kind = 'lifecycle.enterprise_demo'    WHERE kind = 'drip_ent_d3_demo';
UPDATE email_log SET kind = 'lifecycle.enterprise_value'   WHERE kind = 'drip_ent_d7_value_education';
UPDATE email_log SET kind = 'digest.weekly'         WHERE kind = 'digest_weekly';

-- ─── Quiet hours ──────────────────────────────────────────────────
--
-- A push at three in the morning is how an application gets its
-- notifications revoked at the operating-system level, which is a decision
-- nobody reverses. Transactional kinds still go through: someone whose
-- payout failed at 3am would rather know.
--
-- Stored as local hours plus the person's own zone, not as UTC: a talent in
-- Cotonou and one in Paris both mean "not while I am asleep", and their
-- asleep is not the same hour.

ALTER TABLE users
    ADD COLUMN quiet_hours_start SMALLINT
        CHECK (quiet_hours_start IS NULL OR quiet_hours_start BETWEEN 0 AND 23),
    ADD COLUMN quiet_hours_end SMALLINT
        CHECK (quiet_hours_end IS NULL OR quiet_hours_end BETWEEN 0 AND 23),
    -- IANA name, e.g. `Africa/Porto-Novo`. NULL means we do not know, and a
    -- quiet window we cannot place in time is not enforced — guessing UTC
    -- would silence a Beninese talent from 10pm to 6am their time on one
    -- side of the year and not the other.
    ADD COLUMN timezone TEXT;

COMMENT ON COLUMN users.quiet_hours_start IS
    'Local hour push notifications stop. Both bounds set, or neither. '
    'A window that wraps midnight (22 to 7) is normal and handled.';

-- Both or neither: half a window is a window nobody can interpret.
ALTER TABLE users ADD CONSTRAINT users_quiet_hours_complete CHECK (
    (quiet_hours_start IS NULL AND quiet_hours_end IS NULL)
    OR (quiet_hours_start IS NOT NULL AND quiet_hours_end IS NOT NULL AND timezone IS NOT NULL)
);
