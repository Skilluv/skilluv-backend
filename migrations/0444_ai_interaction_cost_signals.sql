-- SKI-298 (T3-01b) — the two facts the AI companion ledger could not answer.
--
-- The ticket asks for a cost projection: cache hit rate and refused
-- requests. Neither was recoverable from `ai_interactions` as it stood.
--
--   * **Cache hits** were written with `tokens_used = 0`, which is also
--     what a worker that reports no token count writes. Deriving the hit
--     rate from that would have silently counted real, billed calls as
--     free ones — the exact number the ticket wants to trust.
--
--   * **Refusals** were not written at all. A request rejected by the
--     burst limiter or the daily quota returned early, so "how often does
--     the guard rail actually fire" had no answer anywhere. That is the
--     one measurement that says whether 10/day is the right number.
--
-- Both are recorded as columns rather than inferred, because an inferred
-- cost metric that is wrong is worse than no cost metric at all.

ALTER TABLE ai_interactions
    ADD COLUMN IF NOT EXISTS cached BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS refusal_kind VARCHAR(16);

ALTER TABLE ai_interactions
    DROP CONSTRAINT IF EXISTS ai_interactions_refusal_kind_check;
ALTER TABLE ai_interactions
    ADD CONSTRAINT ai_interactions_refusal_kind_check CHECK (
        refusal_kind IS NULL OR refusal_kind IN ('burst', 'daily_quota')
    );

-- No code path ever wrote `rate_limited` before this migration, but the
-- status was allowed by 0149's CHECK, so a row could exist. Naming it
-- rather than letting the constraint below fail on it: a migration that
-- aborts on data it could have classified is a migration that needs a
-- human at 3am.
UPDATE ai_interactions
   SET refusal_kind = 'daily_quota'
 WHERE status = 'rate_limited' AND refusal_kind IS NULL;

-- A refusal is exactly the `rate_limited` status; keeping the two in step
-- means the stats endpoint can count either one and get the same answer.
ALTER TABLE ai_interactions
    DROP CONSTRAINT IF EXISTS ai_interactions_refusal_coherent;
ALTER TABLE ai_interactions
    ADD CONSTRAINT ai_interactions_refusal_coherent CHECK (
        (refusal_kind IS NULL AND status <> 'rate_limited')
        OR (refusal_kind IS NOT NULL AND status = 'rate_limited')
    );

-- A cache hit never reaches the worker, so it never spends tokens. Stated
-- rather than assumed: the hit rate is the main cost lever and a row that
-- claims both would make it meaningless.
ALTER TABLE ai_interactions
    DROP CONSTRAINT IF EXISTS ai_interactions_cached_is_free;
ALTER TABLE ai_interactions
    ADD CONSTRAINT ai_interactions_cached_is_free CHECK (
        NOT cached OR tokens_used = 0
    );

-- The stats endpoint always reads a sliding window.
CREATE INDEX IF NOT EXISTS idx_ai_interactions_window
    ON ai_interactions (created_at DESC);
