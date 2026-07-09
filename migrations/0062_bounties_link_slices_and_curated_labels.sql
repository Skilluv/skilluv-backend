-- Phase P1 — Slices deviennent le modèle unifié, bounties = un type de slice.
-- Migration 0062 : ajouter slice_id FK sur oss_bounties + curated_labels sur projects.
--
-- Rationale (voir docs/challenges-target-model-and-roadmap.md partie C phase 1 et H.1) :
--   - `oss_bounties.slice_id` : chaque bounty (existante ou nouvelle) est
--     désormais projetée comme une project_slice. La migration 0063 backfill
--     les bounties existantes. Post-Phase P8, `oss_bounties` disparaît et
--     les slices deviennent la seule table.
--   - `projects.curated_labels` : liste des labels GitHub que le webhook
--     d'ingestion (Phase P2) considère comme déclencheurs de création de slice
--     draft. Défaut = ['good-first-issue', 'help-wanted', 'skilluv-ready'].
--
-- Non-breaking : les colonnes ajoutées sont nullables ou ont des defaults sensés.

-- ═══════════════════════════════════════════════════════════════════
-- oss_bounties.slice_id — lien vers la project_slice correspondante
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE oss_bounties
    ADD COLUMN slice_id UUID REFERENCES project_slices(id) ON DELETE SET NULL;

CREATE INDEX idx_oss_bounties_slice
    ON oss_bounties (slice_id)
    WHERE slice_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- projects.curated_labels — labels GitHub déclencheurs d'ingestion
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE projects
    ADD COLUMN curated_labels TEXT[] NOT NULL
        DEFAULT ARRAY['good-first-issue', 'help-wanted', 'skilluv-ready'];

-- Note : la migration 0063 (backfill) est séparée pour la lisibilité et pour
-- pouvoir être re-jouée indépendamment si besoin (idempotence via WHERE NOT EXISTS).
