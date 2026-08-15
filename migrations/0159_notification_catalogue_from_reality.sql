-- Replace the seeded catalogue with the notifications that actually exist.
--
-- Migration 0155 seeded 28 kinds written from imagination: some the code
-- never sends (`challenge.reviewed`, `badge.earned`), and it missed most of
-- the nineteen the code does send (`guild.kicked`, `interest_accepted`,
-- `tournament.podium`, …). A catalogue that does not match the call sites is
-- worse than none: the settings screen offers toggles for notifications
-- nobody receives, and the ones people actually get are absent from it.
--
-- This is the real list, taken from every delivery call site in the code
-- plus the payment flows added in migrations 0153-0158.
--
-- Naming: `domain.event`, matching the i18n keys. The old code mixed
-- `guild.kicked` with `account_banned` and `new_message`; one convention now.
--
-- `allows_*` and `default_*` answer two different questions, and conflating
-- them is a mistake worth spelling out. `allows_*` is a hard ceiling: the
-- settings screen only renders a toggle where it is TRUE, and `PUT
-- /api/notification-preferences` refuses a channel it forbids. So a FALSE
-- there does not mean "off by default", it means "nobody may ever turn this
-- on" — which is only ever true of a channel that makes no sense at all.
-- Every kind here allows all three. The opinion lives in the defaults.
--
-- Three rules for those defaults:
--
--   * In-app is always on. It is the record of what happened, and the
--     screen a person opens on purpose.
--   * Push defaults on only when someone is waiting, or when the moment is
--     worth interrupting a day for. A notification that buzzes a phone is a
--     claim on attention, and defaulting it on is how an app gets muted.
--   * Email defaults on when the person must act, when they may not open
--     the app for days, or when the thing is rare enough to be worth
--     keeping. Routine social traffic stays in-app.

DELETE FROM notification_preferences;
DELETE FROM notification_kinds;

-- Every kind allows all three channels, for the reason above. The three
-- `default_*` columns are the product decision; read them as
-- in-app / push / email.
INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional) VALUES

    -- ── Money ──────────────────────────────────────────────────────
    -- All transactional. Someone whose payout failed needs to know,
    -- whatever they ticked, and a receipt is not marketing. The two that
    -- do not buzz are the ones nobody is waiting on: money entering
    -- escrow, and a refund that already happened.
    ('payout.sent',            'payments', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, TRUE),
    ('payout.failed',          'payments', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, TRUE),
    ('funds.held',             'payments', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, TRUE),
    ('funds.released',         'payments', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, TRUE),
    ('funds.disputed',         'payments', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, TRUE),
    ('funds.refunded',         'payments', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, TRUE),

    -- ── Account and moderation ─────────────────────────────────────
    -- Being banned is not a preference. Someone locked out has to be told,
    -- and told by email, because they cannot open the app to find out why.
    -- No push either way: a buzz announcing a ban is a taunt.
    ('account.banned',         'account', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, TRUE),
    ('account.unbanned',       'account', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, TRUE),

    -- ── Mentorship ─────────────────────────────────────────────────
    -- A booking and a cancellation both move someone's calendar, so both
    -- travel. `completed` opens the seven-day window in which the mentee
    -- can dispute, which is a deadline and belongs in an inbox.
    ('mentorship.booked',      'mentorship', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, FALSE),
    ('mentorship.completed',   'mentorship', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, FALSE),
    ('mentorship.cancelled',   'mentorship', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, FALSE),

    -- ── Community challenges ───────────────────────────────────────
    -- Someone waiting on a review has usually stopped checking. Email on.
    -- A rejection does not buzz: it lands, it is read when they are ready.
    ('community.challenge_approved', 'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, FALSE),
    ('community.challenge_rejected', 'learning', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, FALSE),

    -- ── Forum and social ───────────────────────────────────────────
    -- The category that makes people mute an application. In-app by
    -- default, and the only two that buzz are the ones where a specific
    -- person is waiting: a direct message, and an accepted answer.
    -- `question_answered` is the exception that earns an email — someone
    -- asked for help and got it, and they may have closed the tab.
    ('forum.answer_accepted',  'social', TRUE, TRUE, TRUE, TRUE, TRUE,  FALSE, FALSE),
    ('forum.reply',            'social', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE),
    ('social.mention',         'social', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE),
    ('dm.received',            'social', TRUE, TRUE, TRUE, TRUE, TRUE,  FALSE, FALSE),

    -- ── Guilds ─────────────────────────────────────────────────────
    -- An invitation expires, so it travels further than the app. Being
    -- removed from a guild is not something to learn from a badge — but it
    -- does not buzz either, for the same reason a ban does not.
    ('guild.invitation',       'guild', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  FALSE),
    ('guild.application',      'guild', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE),
    ('guild.application_decided', 'guild', TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, FALSE),
    ('guild.cofounder_added',  'guild', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    ('guild.role_changed',     'guild', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE),
    ('guild.kicked',           'guild', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  FALSE),
    ('guild.war_proposed',     'guild', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE),

    -- ── Enterprise contact ─────────────────────────────────────────
    -- A company reaching out to a talent, and the talent's answer. Both
    -- sides are waiting on the other, so email is on. A decline does not
    -- need to interrupt anyone's day.
    ('contact.interest_received', 'enterprise', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, FALSE),
    ('contact.interest_accepted', 'enterprise', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, FALSE),
    ('contact.interest_declined', 'enterprise', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, FALSE),

    -- ── Forum, continued ───────────────────────────────────────────
    -- The code distinguishes answering a question from replying to a post,
    -- and it is right to: one is someone waiting for help, the other is a
    -- conversation. They were hidden behind a variable, which is why a first
    -- pass at this catalogue missed them.
    ('forum.question_answered', 'social', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    ('forum.post_replied',      'social', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE),

    -- ── Progress ───────────────────────────────────────────────────
    -- What the proof engine emits when a recompute changes something.
    -- These are the moments the platform exists to produce, so they buzz.
    -- Only the two that are both rare and worth keeping are also emailed:
    -- a rank, and a first verified contribution. A goal is one you set
    -- yourself and check on purpose; a capability arrives alongside the
    -- rank that unlocked it, and two emails for one event is one too many.
    ('rank.promoted',          'learning', TRUE, TRUE, TRUE, TRUE, TRUE, TRUE,  FALSE),
    ('capability.granted',     'learning', TRUE, TRUE, TRUE, TRUE, TRUE, FALSE, FALSE),
    ('badge.awarded',          'learning', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE),
    ('deliverable.first_verified', 'learning', TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, FALSE),
    ('goal.reached',           'learning', TRUE, TRUE, TRUE, TRUE, TRUE, FALSE, FALSE),
    ('tournament.podium',      'learning', TRUE, TRUE, TRUE, TRUE, TRUE, TRUE,  FALSE),

    -- ── Operators ──────────────────────────────────────────────────
    -- Queues someone has to work. Email on by default: an in-app badge
    -- nobody sees is how a queue rots.
    ('admin.review_queued',       'admin', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, FALSE),
    ('admin.fraud_flagged',       'admin', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, FALSE),
    ('admin.payout_needs_replay', 'admin', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, TRUE),
    ('admin.reconciliation_drift','admin', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE, TRUE);

-- Rows written before the catalogue existed carry the old, inconsistent
-- names. Translate the ones that map cleanly so a person's history stays
-- readable, and leave `kind` NULL where it does not — inventing a
-- correspondence would be worse than an absent label.
UPDATE notifications SET kind = 'social.mention'         WHERE notification_type = 'mention.received';
UPDATE notifications SET kind = 'forum.reply'            WHERE notification_type = 'reply.received';
UPDATE notifications SET kind = 'forum.answer_accepted'  WHERE notification_type = 'answer.accepted';
UPDATE notifications SET kind = 'dm.received'            WHERE notification_type IN ('dm.received', 'new_message');
UPDATE notifications SET kind = 'account.banned'         WHERE notification_type = 'account_banned';
UPDATE notifications SET kind = 'account.unbanned'       WHERE notification_type = 'account_unbanned';
UPDATE notifications SET kind = 'community.challenge_approved' WHERE notification_type = 'challenge_approved';
UPDATE notifications SET kind = 'community.challenge_rejected' WHERE notification_type = 'challenge_rejected';
UPDATE notifications SET kind = 'guild.application_decided'    WHERE notification_type = 'guild.application_decision';
UPDATE notifications SET kind = 'guild.war_proposed'     WHERE notification_type = 'guild_war.proposed';
UPDATE notifications SET kind = 'contact.interest_received' WHERE notification_type = 'interest_request_received';
UPDATE notifications SET kind = 'contact.interest_accepted' WHERE notification_type = 'interest_accepted';
UPDATE notifications SET kind = 'contact.interest_declined' WHERE notification_type = 'interest_declined';
UPDATE notifications SET kind = 'forum.question_answered' WHERE notification_type = 'question.answered';
UPDATE notifications SET kind = 'forum.post_replied'      WHERE notification_type = 'post.replied';
UPDATE notifications SET kind = 'rank.promoted'           WHERE notification_type = 'rank_promotion';
UPDATE notifications SET kind = 'capability.granted'      WHERE notification_type = 'capability_granted';
UPDATE notifications SET kind = 'badge.awarded'           WHERE notification_type = 'badge_awarded';
UPDATE notifications SET kind = 'deliverable.first_verified' WHERE notification_type = 'first_verified_deliverable';
UPDATE notifications SET kind = 'goal.reached'            WHERE notification_type = 'milestone_reached';
UPDATE notifications SET kind = notification_type
 WHERE kind IS NULL
   AND notification_type IN (SELECT kind FROM notification_kinds);
