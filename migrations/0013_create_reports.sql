-- User/content reports
CREATE TABLE reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reporter_id UUID NOT NULL REFERENCES users(id),
    target_type VARCHAR(30) NOT NULL CHECK (target_type IN ('user', 'challenge', 'message', 'enterprise')),
    target_id UUID NOT NULL,
    reason VARCHAR(50) NOT NULL CHECK (reason IN ('spam', 'harassment', 'inappropriate', 'cheating', 'fake_profile', 'other')),
    details TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'resolved', 'dismissed')),
    admin_note TEXT,
    handled_by UUID REFERENCES users(id),
    handled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_reports_status ON reports (status, created_at DESC);
CREATE INDEX idx_reports_target ON reports (target_type, target_id);

-- Prevent duplicate pending reports from same user on same target
CREATE UNIQUE INDEX idx_reports_unique_pending
    ON reports (reporter_id, target_type, target_id)
    WHERE status = 'pending';
