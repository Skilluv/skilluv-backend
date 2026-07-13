-- Phase P13.1 — Talent wallets + transactions ledger.
-- Migration 0081.
--
-- Rationale :
--   Jusqu'ici les talents ne gagnaient que des `fragments` (interne). Pour
--   tenir la promesse produit "les entreprises payent les talents en vrai",
--   il faut un wallet réel avec des devises. La partie africaine du projet
--   impose deux canaux :
--   - EUR pour Stripe Connect (talents zone EU/international).
--   - XOF (Franc CFA UEMOA) + autres devises Africa pour Mobile Money.
--
-- Design :
--   - `residency_country` (ISO 3166-1 alpha-2) détermine le canal par défaut.
--     Si `country IN ('CI','SN','BJ','TG','ML','BF','NE','GW')` → XOF Momo.
--     Sinon → EUR Stripe.
--   - Une seule ligne par user (UNIQUE user_id).
--   - Balance stockée en NUMERIC(14,2) — 12 chiffres pour compter jusqu'à
--     999 999 999 999.99, largement suffisant pour un wallet talent.
--
-- Le ledger `talent_transactions` implémente une chaîne de hash :
--   `ledger_hash = SHA256(prev_ledger_hash || id || user_id || delta ||
--                         currency || reason || related_slice_id || created_at)`
--   Rejouable pour audit — si un attaquant modifie une ligne, tous les hash
--   suivants sont invalidés.

CREATE TABLE talent_wallets (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    balance_eur NUMERIC(14,2) NOT NULL DEFAULT 0 CHECK (balance_eur >= 0),
    balance_xof NUMERIC(14,2) NOT NULL DEFAULT 0 CHECK (balance_xof >= 0),
    residency_country VARCHAR(2)
        CHECK (residency_country IS NULL OR length(residency_country) = 2),
    -- Provider Stripe Connect (défini quand le user complète onboarding).
    stripe_account_id VARCHAR(64),
    stripe_kyc_status VARCHAR(20) NOT NULL DEFAULT 'not_started'
        CHECK (stripe_kyc_status IN ('not_started', 'pending', 'verified', 'rejected')),
    -- Provider Mobile Money.
    momo_phone VARCHAR(32),
    momo_phone_verified BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Recherche des wallets avec compte Stripe actif (batch payouts, réconciliation).
CREATE INDEX idx_talent_wallets_stripe
    ON talent_wallets (stripe_account_id)
    WHERE stripe_account_id IS NOT NULL;

-- Recherche par résidence pour ciblage compliance / analytics.
CREATE INDEX idx_talent_wallets_residency
    ON talent_wallets (residency_country)
    WHERE residency_country IS NOT NULL;

CREATE TABLE talent_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Delta positif = credit (payout entrée), négatif = débit (withdraw).
    delta NUMERIC(14,2) NOT NULL,
    currency VARCHAR(3) NOT NULL CHECK (currency IN ('EUR', 'XOF')),
    reason VARCHAR(40) NOT NULL,
    -- Traçabilité vers l'origine (bounty payout, withdraw manuel, refund).
    related_slice_id UUID REFERENCES project_slices(id) ON DELETE SET NULL,
    related_provider_txn_id VARCHAR(128), -- id Stripe transfer ou momo transaction
    notes TEXT,
    -- Ledger hash chain : audit-proof via hash de la ligne précédente.
    prev_ledger_hash BYTEA,
    ledger_hash BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Statement CSV / recherche user → transactions.
CREATE INDEX idx_talent_transactions_user_time
    ON talent_transactions (user_id, created_at DESC);

-- Rapprochement webhook provider ↔ transaction.
CREATE INDEX idx_talent_transactions_provider_txn
    ON talent_transactions (related_provider_txn_id)
    WHERE related_provider_txn_id IS NOT NULL;
