-- Sprint 2 Phase 2 — direct messaging talent↔talent + user-to-user blocks.
--
-- Separate from the existing `conversations` (enterprise↔talent) table because the
-- semantics differ: no interest_request_id, no enterprise_id, no cooldown / decline flow.

-- Conversation between two users. We store the pair canonically (user_a_id < user_b_id)
-- so there's exactly one row per pair regardless of who initiated.
CREATE TABLE dm_conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_a_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    last_message_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (user_a_id <> user_b_id),
    CHECK (user_a_id < user_b_id),  -- enforce canonical ordering
    UNIQUE (user_a_id, user_b_id)
);

CREATE INDEX idx_dm_conversations_a ON dm_conversations (user_a_id, last_message_at DESC);
CREATE INDEX idx_dm_conversations_b ON dm_conversations (user_b_id, last_message_at DESC);

CREATE TABLE dm_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID NOT NULL REFERENCES dm_conversations(id) ON DELETE CASCADE,
    sender_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 4000),
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_dm_messages_conversation ON dm_messages (conversation_id, created_at DESC);
CREATE INDEX idx_dm_messages_unread
    ON dm_messages (conversation_id, sender_id)
    WHERE read_at IS NULL;

-- Talent-to-talent blocks. If A blocks B, B cannot start a new conversation with A,
-- and existing DMs from B are filtered out of A's inbox.
CREATE TABLE user_blocks (
    blocker_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (blocker_id, blocked_id),
    CHECK (blocker_id <> blocked_id)
);

CREATE INDEX idx_user_blocks_blocked ON user_blocks (blocked_id);
