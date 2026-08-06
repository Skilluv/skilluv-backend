-- SKI-81 / SKI-82 (P26 v2 D-05 / D-06) — validator candidacy + admin invite.
--
-- Two paths to the `challenge_validator:{domain}` capability:
--
--   SKI-81 apply  — user requests it themselves; the service checks
--                   stats thresholds (rank >= artisan, ≥10 merged PRs
--                   on that domain, ≥3 repos covered, ≥3 months tenure).
--                   Admin approves via SKI-P26-ADMIN routes.
--
--   SKI-82 invite — admin bypasses stats and creates a pending row that
--                   the user must accept.
--
-- Once accepted+approved, `capabilities_engine::recompute_capabilities_for_user`
-- (P18) is called to grant the capability. Revocation goes through the same
-- engine — this table records the audit trail, not the truth of who currently
-- holds the capability (source of truth is `user_capabilities`).

CREATE TABLE IF NOT EXISTS validator_applications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    domain VARCHAR(20) NOT NULL
        CHECK (domain IN ('code', 'design', 'game', 'security', 'ops', 'ai', 'soft_skills')),
    origin VARCHAR(16) NOT NULL
        CHECK (origin IN ('candidacy', 'invitation')),
    status VARCHAR(16) NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'rejected', 'withdrawn')),
    motivation TEXT,
    -- For invitations: the admin who invited. For candidacies: reviewer.
    admin_actor_id UUID REFERENCES users(id) ON DELETE SET NULL,
    reviewed_at TIMESTAMPTZ,
    review_notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Only one open application per (user, domain, origin). Historic rows
-- (accepted/rejected/withdrawn) can accumulate.
CREATE UNIQUE INDEX IF NOT EXISTS uq_validator_applications_open
    ON validator_applications (user_id, domain, origin)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_validator_applications_by_user
    ON validator_applications (user_id, status, created_at DESC);
