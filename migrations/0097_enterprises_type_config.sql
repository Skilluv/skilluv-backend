-- Phase P24.3 — Config JSONB spécifique par enterprise_type.
-- Migration 0097.
--
-- Rationale :
--   Chaque enterprise_type a des paramètres qui lui sont propres :
--
--     direct_hire         : {} (rien de spécifique pour l'instant)
--     staffing_agency     : {commission_rate: 0.15, brand_white_label: true}
--     remote_international: {eor_provider: 'deel', preferred_currency: 'USD',
--                            timezone_requirement: 'UTC±3'}
--
--   Stockage en JSONB pour flexibilité : chaque type définit ses propres clés,
--   validation applicative côté route (pas de CHECK cross-type en SQL).

ALTER TABLE enterprises
    ADD COLUMN IF NOT EXISTS type_config JSONB NOT NULL DEFAULT '{}'::jsonb;

CREATE INDEX IF NOT EXISTS idx_enterprises_type_config_gin
    ON enterprises USING gin (type_config);
