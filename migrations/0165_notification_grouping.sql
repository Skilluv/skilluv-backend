-- Ten mentions in one thread are one notification. In ten threads, ten.
--
-- ─── The rule ─────────────────────────────────────────────────────
--
-- Collapsing by kind alone would be wrong: ten people mentioning you in ten
-- different discussions is ten things you need to know about, and folding
-- them into "10 mentions" destroys the only information that mattered —
-- where. Collapsing by nothing is what makes a bell worth ignoring.
--
-- So the unit is the **context**: the thing being talked about. Ten
-- mentions in the same thread within a short window become one line that
-- says who and how many. Ten mentions in ten threads stay ten lines.
--
-- ─── The window ───────────────────────────────────────────────────
--
-- Per kind, because "a short while" is not the same everywhere. A burst of
-- replies to one forum post is a conversation and folds over an hour; a
-- direct message an hour after the last one is a new thought and does not
-- fold at all. A NULL window means never group, which is the default and
-- the right answer for anything that carries money or a decision.
--
-- ─── Read is a boundary ───────────────────────────────────────────
--
-- Only unread notifications group. Once someone has seen "Awa mentioned
-- you", the next mention is news again — merging into a line they already
-- read would make it silently change under them, and they would never know
-- the second one happened.

ALTER TABLE notification_kinds
    -- NULL = never group. Seconds, so a window is readable in the row.
    ADD COLUMN group_window_seconds INTEGER
        CHECK (group_window_seconds IS NULL OR group_window_seconds > 0);

COMMENT ON COLUMN notification_kinds.group_window_seconds IS
    'How long an unread notification of this kind stays open to absorbing '
    'another about the same context. NULL never groups.';

ALTER TABLE notifications
    -- `kind:target_type:target_id` — the context, built by the sender.
    -- NULL for anything with no context, which therefore never groups.
    ADD COLUMN group_key TEXT,
    -- How many events this row stands for. Always at least one.
    ADD COLUMN group_count INTEGER NOT NULL DEFAULT 1
        CHECK (group_count >= 1),
    -- The most recent distinct actors, newest first, for "Awa, Kofi and 3
    -- others". Capped in code — a thread with 400 participants must not
    -- carry 400 names in a row someone reads on a phone.
    ADD COLUMN group_actors JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Bumped when a notification absorbs another, so the list can order by
    -- "last thing that happened" rather than "when it started".
    ADD COLUMN updated_at TIMESTAMPTZ;

-- The lookup the sender does on every send: is there an open one for this
-- person and this context? Partial, because a read notification can never
-- match and indexing them would double the index for nothing.
CREATE INDEX idx_notifications_grouping
    ON notifications (user_id, group_key, created_at DESC)
    WHERE read = FALSE AND group_key IS NOT NULL;

-- ─── Windows, per kind ────────────────────────────────────────────
--
-- Only the kinds where a burst about one thing is genuinely one event.
-- Everything else stays NULL: money, decisions, moderation and payouts are
-- each their own line, however many arrive.

UPDATE notification_kinds SET group_window_seconds = CASE kind
    -- A thread is a conversation. An hour of it is one thing to come back
    -- to, and the context is the post.
    WHEN 'forum.reply'            THEN 3600
    WHEN 'forum.post_replied'     THEN 3600
    WHEN 'forum.question_answered' THEN 3600

    -- Several people naming you in the same discussion is one discussion.
    WHEN 'social.mention'         THEN 3600

    -- Messages from the same person, in a burst. Short: two messages ten
    -- minutes apart are two thoughts, and merging them would hide the
    -- second until the first is opened.
    WHEN 'dm.received'            THEN 600

    -- A guild filling up. The context is the guild.
    WHEN 'guild.application'      THEN 7200

    -- Badges arrive in clusters when a recompute runs. Four separate lines
    -- for one recompute reads as a bug.
    WHEN 'badge.awarded'          THEN 900

    -- A moderation queue filling up is one thing to go and work.
    WHEN 'admin.review_queued'    THEN 1800
    WHEN 'admin.fraud_flagged'    THEN 1800

    ELSE NULL
END;
