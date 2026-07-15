-- Phase P24.1 — Types d'entreprise (direct-hire / staffing / remote-international).
-- Migration 0095.
--
-- Rationale :
--   Aujourd'hui `enterprises` traite toutes les organisations comme un seul type
--   (grande boîte qui recrute pour elle-même). Trois profils réels ont des
--   workflows différents :
--
--     - direct_hire         : recrute pour son propre compte (path actuel)
--     - staffing_agency     : recrute pour des clients tiers, invoice en cascade
--     - remote_international : recrute cross-border, EOR + FX + tax withholding
--
--   Le split se fait AU NIVEAU DE L'ORGANISATION (enterprises.enterprise_type),
--   PAS au niveau user. Un enterprise_recruiter (capability P18) reste le même
--   persona ; ce qui change c'est le contexte de son enterprise.
--
--   Colonne posée nullable=NOT NULL avec DEFAULT 'direct_hire' → backfill
--   implicite pour toutes les entreprises existantes (path actuel préservé).

ALTER TABLE enterprises
    ADD COLUMN IF NOT EXISTS enterprise_type VARCHAR(30) NOT NULL DEFAULT 'direct_hire'
        CHECK (enterprise_type IN (
            'direct_hire',
            'staffing_agency',
            'remote_international'
        ));

-- Recherche "toutes les agences de staffing" pour matching talents ou
-- filtrage des workflows spécifiques.
CREATE INDEX IF NOT EXISTS idx_enterprises_type
    ON enterprises (enterprise_type)
    WHERE verified = TRUE;
