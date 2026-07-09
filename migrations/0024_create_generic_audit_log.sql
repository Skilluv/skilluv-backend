-- Generic audit log (Phase 1.18). The existing admin_audit_log stays for backward compat;
-- new actions are written here. Eventually migrate everything to this table.

CREATE TABLE audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_type VARCHAR(20) NOT NULL CHECK (actor_type IN ('user', 'admin', 'system', 'enterprise', 'anonymous')),
    actor_id UUID,  -- nullable for system/anonymous actions
    action VARCHAR(60) NOT NULL,
    target_type VARCHAR(30),
    target_id UUID,
    metadata JSONB,
    ip VARCHAR(45),
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_generic_audit_log_actor ON audit_log (actor_type, actor_id, created_at DESC);
CREATE INDEX idx_generic_audit_log_action ON audit_log (action, created_at DESC);
CREATE INDEX idx_generic_audit_log_target ON audit_log (target_type, target_id, created_at DESC);
-- Automatic cleanup of rows older than 2 years (run by a maintenance cron):
-- DELETE FROM audit_log WHERE created_at < NOW() - INTERVAL '2 years';
