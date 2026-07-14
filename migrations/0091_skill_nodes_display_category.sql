-- Phase P17.2 — display_category sur skill_nodes.
-- Migration 0091.
--
-- Rationale :
--   La spec UX BMAD définit 6 catégories affichables sur les skill patches :
--     Craft · Create · Understand · Operate · Share · Meta
--
--   Le backend a 7 `domain` (code/design/game/security/soft_skills/ai/ops) qui
--   sont plus techniques et moins parlants côté user. Plutôt que refactorer
--   `domain` (breaking change massif), on ajoute une couche de display via
--   `display_category`.
--
--   Mapping deterministic :
--     code       → craft       (bâtir, forger)
--     design     → create      (composer, esthétique)
--     game       → create      (jeux = art + code, penche create côté patch)
--     security   → operate     (opérer, sécuriser, maintenir)
--     ops        → operate     (idem)
--     soft_skills → share      (transmettre, mentorer)
--     ai         → understand  (interpréter, modéliser)
--
--   La 6ᵉ catégorie "meta" (product, growth, strategy, OSS governance) reste
--   à assigner manuellement — pas d'auto-mapping fiable depuis les domains
--   actuels.

ALTER TABLE skill_nodes
    ADD COLUMN IF NOT EXISTS display_category VARCHAR(20) NOT NULL DEFAULT 'craft'
        CHECK (display_category IN ('craft','create','understand','operate','share','meta'));

-- Backfill deterministic depuis domain.
UPDATE skill_nodes SET display_category = CASE domain
    WHEN 'code'        THEN 'craft'
    WHEN 'design'      THEN 'create'
    WHEN 'game'        THEN 'create'
    WHEN 'security'    THEN 'operate'
    WHEN 'ops'         THEN 'operate'
    WHEN 'soft_skills' THEN 'share'
    WHEN 'ai'          THEN 'understand'
    ELSE 'craft'
END;

-- Meta = admin curation. Quelques promotions manuelles pour les skills où
-- meta est plus juste : gouvernance OSS, product, growth. On tag ceux qui
-- matchent des slugs connus (safe, idempotent).
UPDATE skill_nodes SET display_category = 'meta'
WHERE slug IN (
    'open-source-governance',
    'product-thinking',
    'growth-experimentation',
    'strategy',
    'community-building',
    'roadmap-planning'
);

CREATE INDEX IF NOT EXISTS idx_skill_nodes_display_category
    ON skill_nodes (display_category);
