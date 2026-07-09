-- Phase P2.2 — Review queue humaine.
-- Migration 0065 : table `review_tasks` (file d'attente de review humaine).
--
-- Rationale (voir docs/challenges-target-model-and-roadmap.md partie H.2) :
--   Une review_task est créée automatiquement quand un deliverable arrive en
--   verification_status = 'pending' ou 'pending_manual_review'. Un reviewer
--   éligible peut :
--     1. Consulter GET /api/review-queue filtré par domain + priority
--     2. Claim une task (soft-lock 2h)
--     3. Soumettre son verdict via POST /api/deliverables/{id}/reviews
--     4. Le verdict finalise le deliverable et complete la task
--
-- Cold start (12 premiers mois) : GET /api/review-queue reste restreint aux
-- rôles admin / steward. La communauté s'ouvre progressivement (voir H.2).

CREATE TABLE review_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    task_type VARCHAR(30) NOT NULL
        CHECK (task_type IN (
            'verify_deliverable',   -- verdict sur un deliverable
            'verify_slice_claim',   -- arbitrage d'un mismatch author/claimed_by (workflow G.1)
            'arbitrate_dispute'     -- reviewer sénior tranche un désaccord (Phase P5+)
        )),

    -- Une de ces FK est set selon task_type
    deliverable_id UUID REFERENCES deliverables(id) ON DELETE CASCADE,
    slice_id UUID REFERENCES project_slices(id) ON DELETE CASCADE,

    -- État de la task
    status VARCHAR(30) NOT NULL DEFAULT 'open'
        CHECK (status IN (
            'open',                 -- disponible, aucun reviewer assigné
            'claimed',              -- un reviewer travaille dessus (soft-lock 2h)
            'completed',            -- verdict rendu, review inséré
            'escalated',            -- SLA 72h dépassé, admin notifié
            'cancelled'             -- annulée (ex: deliverable revoqué avant review)
        )),
    claimed_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    claimed_at TIMESTAMPTZ,
    claim_expires_at TIMESTAMPTZ,           -- claimed_at + 2h
    completed_at TIMESTAMPTZ,
    completed_review_id UUID REFERENCES reviews(id) ON DELETE SET NULL,

    -- Priorité et SLA (décisions H_Q4 et W4)
    priority SMALLINT NOT NULL DEFAULT 3 CHECK (priority BETWEEN 1 AND 5),
    sla_deadline TIMESTAMPTZ NOT NULL,      -- created_at + 72h
    escalated_at TIMESTAMPTZ,

    -- Filtres pour matching reviewer ↔ task
    primary_domain VARCHAR(30) NOT NULL
        CHECK (primary_domain IN ('code','design','game','security','soft_skills','ai','ops')),
    required_seniority VARCHAR(20) NOT NULL DEFAULT 'any'
        CHECK (required_seniority IN ('any','contribs','impact')),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT review_tasks_has_target
        CHECK (deliverable_id IS NOT NULL OR slice_id IS NOT NULL),
    CONSTRAINT review_tasks_claim_coherent
        CHECK (
            (claimed_by_user_id IS NULL AND claimed_at IS NULL AND claim_expires_at IS NULL)
            OR (claimed_by_user_id IS NOT NULL AND claimed_at IS NOT NULL
                AND claim_expires_at IS NOT NULL)
        )
);

-- Queue publique triée par priority DESC puis ancienneté (FIFO parmi égaux)
CREATE INDEX idx_review_tasks_open_queue
    ON review_tasks (primary_domain, required_seniority, priority DESC, created_at ASC)
    WHERE status = 'open';

-- "Mes tasks en cours" par reviewer
CREATE INDEX idx_review_tasks_claimed_by
    ON review_tasks (claimed_by_user_id, status)
    WHERE claimed_by_user_id IS NOT NULL;

-- Watch SLA + expiration claim (cron horaire)
CREATE INDEX idx_review_tasks_sla_watch
    ON review_tasks (sla_deadline)
    WHERE status IN ('open', 'claimed');

CREATE INDEX idx_review_tasks_claim_expiry
    ON review_tasks (claim_expires_at)
    WHERE status = 'claimed' AND claim_expires_at IS NOT NULL;

-- Trigger updated_at
CREATE OR REPLACE FUNCTION touch_review_tasks_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER review_tasks_updated_at
    BEFORE UPDATE ON review_tasks
    FOR EACH ROW
    EXECUTE FUNCTION touch_review_tasks_updated_at();

-- Note : la logique de création automatique des review_tasks à l'insertion
-- d'un deliverable pending / pending_manual_review est côté service Rust
-- (voir services/review_queue.rs, retrofit dans services/deliverables.rs).
