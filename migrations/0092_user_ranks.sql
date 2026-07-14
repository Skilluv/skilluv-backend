-- Phase P17.4 — Rank system (Apprenti → Ranger → Artisan → Maître → Doyen).
-- Migration 0092.
--
-- Rationale :
--   La spec UX BMAD définit 5 rangs progressifs (chevrons V), 1 seul actif à
--   la fois par user. Seuils :
--
--     APPRENTI : inscription (défaut)
--     RANGER   : 4 deliverables verified
--     ARTISAN  : 11 deliverables + 1 attestation reçue
--     MAÎTRE   : 26 deliverables + 3 attestations
--     DOYEN    : 50 deliverables + statut mentor + capabilities validées
--
--   L'historique doit être conservé — un user qui atteint Ranger puis reste
--   inactif ne perd pas son rang, mais on trace la date d'atteinte pour le
--   profil ("Ranger depuis 2024-03-12").
--
--   Seul le rang courant est queryable rapidement ; les rangs précédents sont
--   historisés dans user_rank_history.

CREATE TABLE user_ranks (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    rank VARCHAR(15) NOT NULL DEFAULT 'apprenti'
        CHECK (rank IN ('apprenti', 'ranger', 'artisan', 'maitre', 'doyen')),
    achieved_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    previous_rank VARCHAR(15)
        CHECK (previous_rank IS NULL OR
               previous_rank IN ('apprenti', 'ranger', 'artisan', 'maitre', 'doyen'))
);

CREATE INDEX idx_user_ranks_by_rank
    ON user_ranks (rank, achieved_at DESC);

-- Historique des transitions (audit + timeline profil).
CREATE TABLE user_rank_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    from_rank VARCHAR(15),
    to_rank VARCHAR(15) NOT NULL,
    achieved_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reason TEXT
);

CREATE INDEX idx_user_rank_history_by_user
    ON user_rank_history (user_id, achieved_at DESC);

-- Backfill : chaque user existant démarre à Apprenti.
INSERT INTO user_ranks (user_id, rank)
SELECT id, 'apprenti' FROM users
ON CONFLICT DO NOTHING;
