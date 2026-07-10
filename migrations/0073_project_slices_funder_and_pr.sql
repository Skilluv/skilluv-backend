-- Phase P9.2 — Fusion `oss_bounties` + `oss_bounty_claims` dans project_slices.
-- Migration 0073 : ajout des colonnes de funder + PR tracking sur project_slices,
-- puis backfill depuis les tables legacy avant leur drop en migration 0074.
--
-- Nouveauté modèle :
--   - `funder_enterprise_id` : l'enterprise B2B qui a mis les crédits en séquestre.
--     NULL pour les slices "vanilla" issues du webhook / import partenaire /
--     création manuelle (pas de payout financier attendu, seuls les fragments).
--   - `funded_by_user_id` : le user (dans l'enterprise) qui a créé la bounty.
--     Distinct de `created_by_user_id` — ce dernier peut être un steward humain
--     Skilluv, funded_by_user_id est spécifique au flow B2B.
--   - `pr_url`, `pr_number`, `pr_submitted_at`, `merged_at`, `paid_at` : traçage
--     du cycle de vérification via PR mergée (workflow G.1). Ces champs étaient
--     sur oss_bounty_claims — désormais persistés directement sur la slice
--     puisque la contrainte "un seul claim par slice" (0058) est déjà en place.

ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS funder_enterprise_id UUID
        REFERENCES enterprises(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS funded_by_user_id UUID
        REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS pr_url TEXT,
    ADD COLUMN IF NOT EXISTS pr_number INTEGER,
    ADD COLUMN IF NOT EXISTS pr_submitted_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS merged_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS paid_at TIMESTAMPTZ;

-- Recherche "mes bounties" côté enterprise dashboard
CREATE INDEX IF NOT EXISTS idx_slices_funder
    ON project_slices (funder_enterprise_id, status)
    WHERE funder_enterprise_id IS NOT NULL;

-- Backfill funder info depuis oss_bounties liées
UPDATE project_slices ps
SET funder_enterprise_id = ob.enterprise_id,
    funded_by_user_id = ob.posted_by_user_id
FROM oss_bounties ob
WHERE ob.slice_id = ps.id
  AND ps.funder_enterprise_id IS NULL;

-- Backfill claim + PR info depuis oss_bounty_claims.
-- On prend le claim "actif" (priorité merged > pr_submitted > claimed) via
-- DISTINCT ON pour chaque bounty. Note : le claim expiré/abandonned n'est
-- pas ramené (comportement conservateur).
UPDATE project_slices ps
SET claimed_by_user_id = latest.user_id,
    claimed_at = latest.claimed_at,
    pr_url = latest.pull_request_url,
    pr_number = latest.pull_request_number,
    pr_submitted_at = latest.pr_submitted_at,
    merged_at = latest.merged_at,
    paid_at = CASE ob.status WHEN 'paid' THEN COALESCE(latest.merged_at, NOW()) ELSE NULL END
FROM oss_bounties ob
JOIN LATERAL (
    SELECT c.user_id, c.claimed_at, c.pull_request_url, c.pull_request_number,
           c.pr_submitted_at, c.merged_at, c.status
    FROM oss_bounty_claims c
    WHERE c.bounty_id = ob.id
    ORDER BY CASE c.status
                 WHEN 'merged' THEN 1
                 WHEN 'pr_submitted' THEN 2
                 WHEN 'claimed' THEN 3
                 ELSE 4
             END,
             c.claimed_at DESC NULLS LAST
    LIMIT 1
) latest ON TRUE
WHERE ob.slice_id = ps.id
  AND ps.claimed_by_user_id IS NULL;
