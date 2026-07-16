-- Phase BE-P26 — Ledger unifié des marges Skilluv (audit trail revenus).
-- Migration 0100.
--
-- Rationale :
--   Les marges Skilluv (fees sur mentor sessions 20%, bounty 8%, futures API
--   fees, sponsorships) sont aujourd'hui dispersées : mentor via colonne
--   `mentorship_sessions.price_platform_cents`, bounty (à activer par cette
--   phase) sans persistance.
--
--   Cette table `platform_revenues` centralise toutes les prises de marge
--   Skilluv en un ledger unique, queryable pour :
--     - Reporting mensuel MRR ("combien de fees mentor ce mois ?")
--     - Audit externe (SOC/compta) — chaque marge est attribuable
--     - Analyse produit (quel levier rapporte plus ?)
--     - Base pour futures projections revenue post-launch
--
--   NOTE : cette table ne REMPLACE PAS la comptabilité tenue en amont
--   (`credit_transactions` pour flow enterprise, `mentorship_sessions` pour
--   mentor). Elle DUPLIQUE partiellement pour créer une vue unifiée. La source
--   de vérité comptable reste les tables métier.

CREATE TABLE platform_revenues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Source du revenu : levier produit qui a généré la marge.
    source VARCHAR(20) NOT NULL
        CHECK (source IN (
            'bounty',            -- fee sur transfer bounty enterprise → talent
            'mentor_session',    -- fee 20% sur session mentor payée
            'api_metered',       -- fee mensuel API publique (post-MVP)
            'sponsored_challenge', -- prix flat sponsorship (post-MVP)
            'other'              -- fallback, à typer plus tard
        )),

    -- Références vers la source (nullable selon type).
    source_slice_id UUID REFERENCES project_slices(id) ON DELETE SET NULL,
    source_session_id UUID REFERENCES mentorship_sessions(id) ON DELETE SET NULL,

    -- Acteurs impliqués (traçabilité).
    related_talent_id UUID REFERENCES users(id) ON DELETE SET NULL,
    related_enterprise_id UUID REFERENCES enterprises(id) ON DELETE SET NULL,

    -- Montant. `amount_credits` pour bounty (unité crédits Skilluv B2B),
    -- `amount_fiat_cents` pour mentor/API (unité cents dans currency donné).
    -- Au moins l'une des 2 doit être renseignée.
    amount_credits NUMERIC(10, 2),
    amount_fiat_cents BIGINT,
    currency VARCHAR(10),

    fee_rate_bps INTEGER NOT NULL,           -- Ex: 800 = 8%, 2000 = 20%
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    notes TEXT,

    CONSTRAINT platform_revenues_amount_present
        CHECK (amount_credits IS NOT NULL OR amount_fiat_cents IS NOT NULL)
);

-- Query dashboard : "revenues by source over time"
CREATE INDEX idx_platform_revenues_source_time
    ON platform_revenues (source, occurred_at DESC);

-- Query "revenue by enterprise" (pour billing/reporting client)
CREATE INDEX idx_platform_revenues_enterprise
    ON platform_revenues (related_enterprise_id, occurred_at DESC)
    WHERE related_enterprise_id IS NOT NULL;

-- Documentation
COMMENT ON TABLE platform_revenues IS
'BE-P26 — Ledger unifié des marges Skilluv (fee mentor 20%, bounty 8%, futures API/sponsored). Source de reporting revenue, PAS source de vérité comptable (celle-ci reste dans les tables métier credit_transactions / mentorship_sessions).';
