-- Phase IA-D — Table ai_call_log pour audit trail + coût monitoring gRPC IA.
-- Migration 0101.
--
-- Rationale (docs/BACKEND-INTEGRATION.md §9) :
--   Chaque appel gRPC vers skilluv-ai doit être loggé pour :
--     - Reconstitution ex-post d'un incident (indépendant Grafana rétention 30j).
--     - Analyse coût LLM par méthode / user / semaine.
--     - Détection abus (user avec 200 ReviewCode/jour = probablement bot).
--     - Traçabilité audit : quel `model_version` a produit tel verdict.
--
--   TTL 90 jours (à activer via cron post-MVP). Au-delà, agrégats en analytics.

CREATE TABLE ai_call_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    method VARCHAR(40) NOT NULL,
        -- 'ReviewCode' | 'GenerateChallenge' | 'GenerateVariant'
        -- | 'AnalyzePerformance' | 'SuggestCareerPath' | 'CheckPlagiarism'
    submission_id UUID,           -- pour ReviewCode + CheckPlagiarism
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    latency_ms INTEGER NOT NULL,
    status VARCHAR(30) NOT NULL
        CHECK (status IN ('ok', 'unavailable', 'internal', 'business_failure', 'timeout')),
    model_version VARCHAR(50),    -- ex: 'claude-opus-4-7', renseigné par l'IA
    error_message TEXT,           -- non-null si status != 'ok'
    called_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_call_log_called_at ON ai_call_log (called_at DESC);
CREATE INDEX idx_ai_call_log_method_status ON ai_call_log (method, status, called_at DESC);
CREATE INDEX idx_ai_call_log_user ON ai_call_log (user_id, called_at DESC)
    WHERE user_id IS NOT NULL;

COMMENT ON TABLE ai_call_log IS
'IA-D — Audit trail des appels gRPC vers skilluv-ai. Append-only par contrat applicatif (pas de REVOKE PG car table non tenant-scoped). TTL 90j via cron post-MVP.';
