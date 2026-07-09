-- Phase 4.12 — Web Push (VAPID) subscription storage.

CREATE TABLE push_subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    endpoint TEXT NOT NULL,
    p256dh_key TEXT NOT NULL,
    auth_secret TEXT NOT NULL,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_success_at TIMESTAMPTZ,
    last_failure_at TIMESTAMPTZ,
    failure_count INTEGER NOT NULL DEFAULT 0,
    UNIQUE (user_id, endpoint)
);

CREATE INDEX idx_push_subs_user ON push_subscriptions (user_id);
