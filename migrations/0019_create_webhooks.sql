-- Webhooks
CREATE TABLE webhooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    url VARCHAR(500) NOT NULL,
    secret VARCHAR(255) NOT NULL,
    events TEXT[] NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    last_triggered_at TIMESTAMPTZ,
    failure_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_webhooks_user ON webhooks (user_id);

-- Webhook delivery log
CREATE TABLE webhook_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event VARCHAR(50) NOT NULL,
    payload JSONB NOT NULL,
    response_status INTEGER,
    response_body TEXT,
    success BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_webhook_deliveries_webhook ON webhook_deliveries (webhook_id, created_at DESC);
