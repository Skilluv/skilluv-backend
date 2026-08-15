-- Where a notification's button sends the reader.
--
-- The email frame has supported a call to action from the start and nothing
-- ever filled it in, so every email went out as a statement with no way to
-- act on it. Someone told their payout arrived had to find the wallet page
-- themselves; most people do not, and the notification may as well not have
-- been sent.
--
-- The path lives with the kind rather than at the call site. A call site
-- decides *that* something happened; where the reader should go to see it is
-- a property of the kind, the same as its title and its channels — and
-- keeping it here means one place to fix when a route changes, instead of
-- twenty-seven.
--
-- Placeholders are filled from the notification's payload: `/guilds/{guild_id}`
-- with `{"guild_id": "..."}` becomes `/guilds/abc`. A placeholder with no
-- matching key leaves the button out entirely rather than linking to a
-- broken URL — a dead button is worse than none.

ALTER TABLE notification_kinds
    ADD COLUMN cta_path TEXT;

COMMENT ON COLUMN notification_kinds.cta_path IS
    'Frontend path for the email button, relative to the app root. '
    'Placeholders like {guild_id} are filled from the notification payload; '
    'an unfilled placeholder suppresses the button. NULL means no button.';

UPDATE notification_kinds SET cta_path = CASE kind
    -- Money: the wallet, where the amount and its state are visible.
    WHEN 'payout.sent'            THEN '/wallet'
    WHEN 'payout.failed'          THEN '/wallet'
    WHEN 'funds.held'             THEN '/wallet'
    WHEN 'funds.released'         THEN '/wallet'
    WHEN 'funds.disputed'         THEN '/wallet'
    WHEN 'funds.refunded'         THEN '/wallet'

    -- Account: nowhere useful to send someone who is banned, so no button.
    WHEN 'account.unbanned'       THEN '/'

    -- Mentorship: the session itself. `completed` is the one that matters —
    -- its button is what releases the mentor's money early.
    WHEN 'mentorship.booked'      THEN '/mentorship/sessions/{session_id}'
    WHEN 'mentorship.completed'   THEN '/mentorship/sessions/{session_id}'
    WHEN 'mentorship.cancelled'   THEN '/mentorship/sessions'

    WHEN 'community.challenge_approved' THEN '/challenges/{challenge_id}'
    WHEN 'community.challenge_rejected' THEN '/challenges/{challenge_id}'

    WHEN 'forum.answer_accepted'  THEN '/forum/posts/{post_id}'
    WHEN 'forum.question_answered' THEN '/forum/posts/{post_id}'
    WHEN 'forum.post_replied'     THEN '/forum/posts/{post_id}'
    WHEN 'forum.reply'            THEN '/forum/posts/{post_id}'
    WHEN 'social.mention'         THEN '/notifications'
    WHEN 'dm.received'            THEN '/messages/{conversation_id}'

    WHEN 'guild.invitation'       THEN '/guilds/{guild_slug}'
    WHEN 'guild.application'      THEN '/guilds/{guild_slug}/applications'
    WHEN 'guild.application_decided' THEN '/guilds/{guild_slug}'
    WHEN 'guild.cofounder_added'  THEN '/guilds/{guild_slug}'
    WHEN 'guild.role_changed'     THEN '/guilds/{guild_slug}'
    WHEN 'guild.war_proposed'     THEN '/guilds/{guild_slug}/wars'

    WHEN 'contact.interest_received' THEN '/contact/requests'
    WHEN 'contact.interest_accepted' THEN '/contact/requests'
    WHEN 'contact.interest_declined' THEN '/contact/requests'

    WHEN 'rank.promoted'          THEN '/profile'
    WHEN 'capability.granted'     THEN '/profile'
    WHEN 'badge.awarded'          THEN '/profile'
    WHEN 'deliverable.first_verified' THEN '/profile'
    -- The point of reaching a goal is to set the next one.
    WHEN 'goal.reached'           THEN '/profile/goals'
    WHEN 'tournament.podium'      THEN '/tournaments'

    -- Operator queues. The point of these is to open the queue.
    WHEN 'admin.review_queued'       THEN '/admin/moderation'
    WHEN 'admin.fraud_flagged'       THEN '/admin/fraud'
    WHEN 'admin.payout_needs_replay' THEN '/admin/payouts'
    WHEN 'admin.reconciliation_drift' THEN '/admin/payouts/reconciliation'

    -- `account.banned` and `guild.kicked` deliberately have none: there is
    -- nothing to click, and a button would be a taunt.
    ELSE NULL
END;
