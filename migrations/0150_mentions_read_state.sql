-- SKI-286 — make `mentions` usable as a real inbox.
--
-- The table has existed since migration 0025 but nothing ever wrote to it:
-- `services::social::record_mentions` had no caller, and there were no
-- endpoints. This migration adds what an inbox needs on top of the
-- existing shape.
--
-- ## Read state
--
-- `read_at` turns the list into an inbox rather than a log. NULL means
-- unread, which is the state every new row starts in.
--
-- ## Idempotency
--
-- The ticket requires that editing content to add a new @username creates
-- a mention for that user only — re-saving must not duplicate the ones
-- already recorded. A unique index on (mentioned_user, source, author)
-- expresses that directly, so the insert can be ON CONFLICT DO NOTHING and
-- the rule holds even if two edits race.
--
-- `author_id` is part of the key because the same content can, in
-- principle, be edited by a moderator: their edit should not silently
-- overwrite the original author's attribution.
--
-- ## Source types
--
-- Constrained now that there are exactly four writing surfaces. Existing
-- rows are cleaned first: the column was free-form and unvalidated, and
-- since nothing wrote to it in production the delete is a no-op there
-- while keeping local databases migratable.

DELETE FROM mentions
 WHERE source_type NOT IN ('forum_post', 'comment', 'slice_diary', 'message');

ALTER TABLE mentions
    ADD CONSTRAINT mentions_source_type_known
    CHECK (source_type IN ('forum_post', 'comment', 'slice_diary', 'message'));

ALTER TABLE mentions
    ADD COLUMN IF NOT EXISTS read_at TIMESTAMPTZ;

-- One mention per (target, source, author). Deduplicates re-saves.
CREATE UNIQUE INDEX IF NOT EXISTS idx_mentions_unique
    ON mentions (mentioned_user_id, source_type, source_id, author_id);

-- The inbox read path: my mentions, newest first, optionally unread only.
CREATE INDEX IF NOT EXISTS idx_mentions_inbox
    ON mentions (mentioned_user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_mentions_unread
    ON mentions (mentioned_user_id, created_at DESC)
    WHERE read_at IS NULL;
