-- Admin audit log
CREATE TABLE admin_audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    admin_id UUID NOT NULL REFERENCES users(id),
    action VARCHAR(50) NOT NULL,
    target_type VARCHAR(30),
    target_id UUID,
    details JSONB,
    ip_address VARCHAR(45),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_log_admin ON admin_audit_log (admin_id, created_at DESC);
CREATE INDEX idx_audit_log_action ON admin_audit_log (action, created_at DESC);
