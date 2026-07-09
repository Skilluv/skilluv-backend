-- Interest requests from enterprises to talents
CREATE TABLE interest_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    sender_id UUID NOT NULL REFERENCES users(id),
    talent_id UUID NOT NULL REFERENCES users(id),
    status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'declined')),
    initial_message TEXT NOT NULL,
    declined_at TIMESTAMPTZ,
    accepted_at TIMESTAMPTZ,
    cooldown_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_interest_requests_talent ON interest_requests (talent_id, status);
CREATE INDEX idx_interest_requests_enterprise ON interest_requests (enterprise_id, created_at DESC);

-- Prevent duplicate pending requests from same enterprise to same talent
CREATE UNIQUE INDEX idx_interest_requests_unique_pending
    ON interest_requests (enterprise_id, talent_id)
    WHERE status = 'pending';

-- Conversation threads (created when interest request is accepted)
CREATE TABLE conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    interest_request_id UUID NOT NULL UNIQUE REFERENCES interest_requests(id) ON DELETE CASCADE,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    talent_id UUID NOT NULL REFERENCES users(id),
    closed BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_conversations_talent ON conversations (talent_id);
CREATE INDEX idx_conversations_enterprise ON conversations (enterprise_id);

-- Messages within a conversation
CREATE TABLE messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    sender_id UUID NOT NULL REFERENCES users(id),
    content TEXT NOT NULL,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_messages_conversation ON messages (conversation_id, created_at);

-- Enterprise blocks by talent
CREATE TABLE enterprise_blocks (
    talent_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (talent_id, enterprise_id)
);
