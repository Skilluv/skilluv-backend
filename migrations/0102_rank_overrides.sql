-- ADM-M5 — Table rank_overrides pour historisation des overrides admin.
-- Migration 0102.
--
-- Rationale :
--   Un admin peut, sur cas exceptionnel (bug proof engine, doyen d'honneur,
--   rétrogradation post-modération), forcer un rank pour un user. On garde
--   l'historique — pas d'endpoint delete, seul un nouvel override peut
--   annuler l'effet du précédent.
--
--   Écrit *en plus* de user_ranks (la source de vérité). Le PATCH sur
--   user_ranks reste géré par la route admin (transaction unique).
--
--   Enum aligné sur mig 0092 : apprenti|ranger|artisan|maitre|doyen.

CREATE TABLE rank_overrides (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    admin_id UUID NOT NULL REFERENCES users(id) ON DELETE SET NULL,
    old_rank VARCHAR(15) NOT NULL
        CHECK (old_rank IN ('apprenti', 'ranger', 'artisan', 'maitre', 'doyen')),
    new_rank VARCHAR(15) NOT NULL
        CHECK (new_rank IN ('apprenti', 'ranger', 'artisan', 'maitre', 'doyen')),
    reason TEXT NOT NULL CHECK (length(reason) >= 8),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_rank_overrides_by_user
    ON rank_overrides (user_id, created_at DESC);

CREATE INDEX idx_rank_overrides_by_admin
    ON rank_overrides (admin_id, created_at DESC);

COMMENT ON TABLE rank_overrides IS
    'ADM-M5 — audit log des overrides admin sur user_ranks. Append-only, jamais mutable.';
