-- Content strategy foundation — Bonjour Skilluv onboarding tracking (2026-07-21 decision).
--
-- Rationale: the Bonjour Skilluv challenge is every new user's first commit.
-- Flow:
--   1. User signs up + picks orientation
--   2. User triggers POST /api/onboarding/bonjour-skilluv/start
--   3. Backend forks skilluv-community/starter-{stack} onto user's GitHub via API
--   4. User edits HELLO.md locally, commits, pushes, opens PR (main → showcase)
--   5. GitHub webhook detects PR on user's fork → status transitions to hello_committed
--   6. Mentor / auto-review completes → status = completed, badge unlocked, hello_wall_entries created
--
-- See content strategy doc §9 for full flow description.

CREATE TABLE onboarding_bonjour_skilluv (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,

    -- Which starter template was forked
    starter_slug VARCHAR(60) NOT NULL
        CHECK (starter_slug ~ '^starter-[a-z0-9-]+$'),

    -- Fork identity on GitHub
    fork_full_name VARCHAR(180) NOT NULL,           -- e.g. "amina/starter-fullstack-rust"
    fork_html_url TEXT NOT NULL
        CHECK (fork_html_url ~ '^https://github\.com/[a-zA-Z0-9_-]+/[a-zA-Z0-9_.-]+$'),
    github_fork_id BIGINT NOT NULL UNIQUE,

    -- Progression status
    status VARCHAR(30) NOT NULL DEFAULT 'forked'
        CHECK (status IN ('forked', 'hello_committed', 'pr_opened', 'completed', 'abandoned')),

    -- PR tracking (populated by webhook when user opens PR)
    pr_number INTEGER,
    pr_url TEXT
        CHECK (pr_url IS NULL OR pr_url ~ '^https://github\.com/[a-zA-Z0-9_-]+/[a-zA-Z0-9_.-]+/pull/\d+$'),

    -- Linked artifacts (populated on completion)
    deliverable_id UUID REFERENCES deliverables(id) ON DELETE SET NULL,

    -- Timestamps for observability + funnel analytics
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    pr_opened_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,

    -- Coherence
    CHECK (
        (status IN ('forked', 'hello_committed') AND pr_number IS NULL AND pr_url IS NULL)
        OR (status IN ('pr_opened', 'completed') AND pr_number IS NOT NULL AND pr_url IS NOT NULL)
        OR (status = 'abandoned')
    ),
    CHECK (completed_at IS NULL OR status = 'completed')
);

-- For webhook resolution: given a fork_full_name, find the onboarding row
CREATE INDEX idx_onboarding_bonjour_fork_full_name
    ON onboarding_bonjour_skilluv (fork_full_name);

-- For funnel analytics
CREATE INDEX idx_onboarding_bonjour_status
    ON onboarding_bonjour_skilluv (status, started_at DESC);

COMMENT ON TABLE onboarding_bonjour_skilluv IS
    'Tracks each user progression through the Bonjour Skilluv onboarding challenge. One row per user (PK on user_id). Only created when user calls POST /api/onboarding/bonjour-skilluv/start, which forks a skilluv-community/starter-* template on their GitHub. Webhook completes the flow when the user opens a PR touching HELLO.md on their fork.';

COMMENT ON COLUMN onboarding_bonjour_skilluv.starter_slug IS
    'Which skilluv-community/starter-* template was forked. Auto-selected from user orientation but user can override via query param.';

COMMENT ON COLUMN onboarding_bonjour_skilluv.status IS
    'forked = fork created, waiting on user to commit HELLO.md. hello_committed = local commit detected (optional intermediate). pr_opened = PR opened by user (webhook detected). completed = review passed, badge unlocked, hello_wall entry created. abandoned = user gave up (never explicitly set, inferred by cleanup job after 90 days idle).';
