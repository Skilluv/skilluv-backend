-- SKI-44 (Post-MVP T3-01) — disclosed AI learning companion.
--
-- ChatGPT is the invisible competitor. A platform whose currency is
-- verifiable proof cannot pretend its users are not already asking an LLM
-- for help; it can only choose whether that help is disclosed or hidden.
-- This table is the disclosure.
--
-- ## Relationship to `ai_call_log`
--
-- `ai_call_log` (migration 0101) is operational telemetry: latency, status,
-- model version, one row per gRPC call, kept for debugging and cost
-- tracking. It is not a record of what a *learner* asked, it has no
-- content, and it is not something a deliverable can cite.
--
-- `ai_interactions` is the disclosure ledger: what was asked, in what
-- capacity, and whether it has since been attached to submitted work. Both
-- rows are written for a companion call; they answer different questions
-- and have different retention needs.
--
-- ## Disclosure lifecycle
--
-- 1. A companion call inserts a row with `disclosed_on_deliverable_id`
--    NULL — help received, not yet used for anything.
-- 2. When the user submits a deliverable, every undisclosed interaction
--    from the preceding window is stamped with that deliverable's id and
--    merged into `deliverables.verification_signal.ai_companion`.
--
-- The window is what makes this honest without being absurd: attaching
-- every interaction a user ever had would make the disclosure meaningless,
-- and attaching none would make it a lie.

CREATE TABLE IF NOT EXISTS ai_interactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    interaction_type VARCHAR(24) NOT NULL
        CHECK (interaction_type IN (
            'explain',
            'generate_exercises',
            'pre_review',
            'debug_help'
        )),
    -- The learner's question, truncated by the service layer. Stored so a
    -- disclosure can be audited rather than merely asserted.
    prompt TEXT NOT NULL CHECK (length(prompt) <= 4000),
    -- Optional skill the interaction was about.
    skill_slug VARCHAR(80),
    -- Whether the exchange reached the AI worker at all.
    status VARCHAR(20) NOT NULL DEFAULT 'ok'
        CHECK (status IN ('ok', 'unavailable', 'rate_limited', 'error')),
    -- Copied verbatim from the worker's CompanionResponse.
    disclosure_label TEXT NOT NULL DEFAULT '',
    model_version VARCHAR(50),
    tokens_used INTEGER NOT NULL DEFAULT 0 CHECK (tokens_used >= 0),
    -- Set once the interaction has been attached to submitted work.
    disclosed_on_deliverable_id UUID REFERENCES deliverables(id) ON DELETE SET NULL,
    disclosed_at TIMESTAMPTZ,
    -- SHA-256 of the normalized request, for the response cache. Repeated
    -- identical questions are answered from cache rather than re-billed.
    request_hash CHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT ai_interactions_disclosure_coherent CHECK (
        (disclosed_on_deliverable_id IS NULL AND disclosed_at IS NULL)
        OR (disclosed_on_deliverable_id IS NOT NULL AND disclosed_at IS NOT NULL)
    )
);

-- Quota accounting and the user's own history.
CREATE INDEX IF NOT EXISTS idx_ai_interactions_by_user
    ON ai_interactions (user_id, created_at DESC);

-- The disclosure sweep: undisclosed interactions for one user.
CREATE INDEX IF NOT EXISTS idx_ai_interactions_undisclosed
    ON ai_interactions (user_id, created_at DESC)
    WHERE disclosed_on_deliverable_id IS NULL;

-- Cache lookup.
CREATE INDEX IF NOT EXISTS idx_ai_interactions_request_hash
    ON ai_interactions (request_hash)
    WHERE request_hash IS NOT NULL;
