-- Phase P2 — Deliverables + Reviews.
-- Migration 0064 : tables `reviews` et `review_metrics`.
--
-- Rationale (voir docs/challenges-target-model-and-roadmap.md sections B.7, B.8,
-- et partie G.3/H.2) :
--   - `reviews` : chaque verdict d'un reviewer humain sur un deliverable.
--     Un reviewer ne peut donner qu'un verdict par deliverable (UNIQUE).
--   - `review_metrics` : agrégat calculé par reviewer, alimente reputation_score
--     via la formule Q4 : 0.5 + 0.30*accuracy + 0.20*rejection_relevance
--                       - 0.05*abandonment (clamped [0,1], bootstrap 0.5 avant 5 reviews)
--
-- Ces tables sont utilisées par :
--   - P2.1 (cette phase) : deliverable.verified_by_user_id peut pointer sur
--     un reviewer si human_review. La table reviews n'est pas encore alimentée
--     activement (reviewer flow arrive en P2.2 avec review_queue).
--   - P2.2 : le workflow reviewer humain complet + submit_review.

-- ═══════════════════════════════════════════════════════════════════
-- Table : reviews
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE reviews (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    deliverable_id UUID NOT NULL REFERENCES deliverables(id) ON DELETE CASCADE,
    reviewer_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    verdict VARCHAR(20) NOT NULL
        CHECK (verdict IN ('approve','request_changes','reject','abstain')),
    body TEXT NOT NULL,

    -- Contexte figé au moment de la review (utile pour l'audit + les stats)
    reviewer_phase_at_time VARCHAR(20)
        CHECK (reviewer_phase_at_time IS NULL OR reviewer_phase_at_time IN (
            'bootstrap','katas','contribs','impact'
        )),
    reviewer_reputation_at_time DECIMAL(3,2)
        CHECK (reviewer_reputation_at_time IS NULL
               OR (reviewer_reputation_at_time >= 0.0
                   AND reviewer_reputation_at_time <= 1.0)),
    time_spent_seconds INTEGER CHECK (time_spent_seconds IS NULL OR time_spent_seconds >= 0),

    -- Récompense reviewer (rejet pertinent aussi récompensé, décision Q4)
    fragments_awarded INTEGER NOT NULL DEFAULT 0 CHECK (fragments_awarded >= 0),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Un reviewer ne peut pas review 2 fois le même deliverable
    UNIQUE (deliverable_id, reviewer_user_id)
);

CREATE INDEX idx_reviews_deliverable
    ON reviews (deliverable_id, created_at);

CREATE INDEX idx_reviews_reviewer
    ON reviews (reviewer_user_id, created_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- Table : review_metrics
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE review_metrics (
    reviewer_user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,

    total_reviews INTEGER NOT NULL DEFAULT 0 CHECK (total_reviews >= 0),
    approved_count INTEGER NOT NULL DEFAULT 0 CHECK (approved_count >= 0),
    rejected_count INTEGER NOT NULL DEFAULT 0 CHECK (rejected_count >= 0),
    request_changes_count INTEGER NOT NULL DEFAULT 0 CHECK (request_changes_count >= 0),
    abstain_count INTEGER NOT NULL DEFAULT 0 CHECK (abstain_count >= 0),

    -- Métriques dérivées (recomputées par un job nightly)
    accuracy_score DECIMAL(3,2)
        CHECK (accuracy_score IS NULL
               OR (accuracy_score >= 0.0 AND accuracy_score <= 1.0)),
    rejection_relevance_score DECIMAL(3,2)
        CHECK (rejection_relevance_score IS NULL
               OR (rejection_relevance_score >= 0.0 AND rejection_relevance_score <= 1.0)),
    abandonment_penalty DECIMAL(3,2) NOT NULL DEFAULT 0.0
        CHECK (abandonment_penalty >= 0.0 AND abandonment_penalty <= 1.0),

    avg_response_time_seconds INTEGER,
    endorsement_count INTEGER NOT NULL DEFAULT 0,

    -- Réputation composée via formule Q4 (session 2026-07-09) :
    --   0.5 + 0.30 * accuracy + 0.20 * rejection_relevance - 0.05 * abandonment
    --   Clamped [0.0, 1.0], bootstrap 0.5 avant 5 reviews complétées.
    reputation_score DECIMAL(3,2) NOT NULL DEFAULT 0.5
        CHECK (reputation_score >= 0.0 AND reputation_score <= 1.0),

    last_recomputed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_review_metrics_reputation
    ON review_metrics (reputation_score DESC);

-- Trigger classique de maintenance
CREATE OR REPLACE FUNCTION touch_review_metrics_recomputed_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.last_recomputed_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER review_metrics_touch
    BEFORE UPDATE ON review_metrics
    FOR EACH ROW
    EXECUTE FUNCTION touch_review_metrics_recomputed_at();
