-- Phase P9.2 — DROP des tables legacy `oss_bounties` + `oss_bounty_claims`.
-- Migration 0074.
--
-- Prérequis :
--   - Migration 0073 a backfill funder + PR info sur project_slices.
--   - routes/bounties.rs (P9.2) lit/écrit uniquement project_slices.
--
-- Irréversible. Les données historiques importantes (funder, PR URL, merged_at,
-- statut) ont été portées sur project_slices via 0073. Les données non portées :
--   - `oss_bounty_claims` des claims non-actifs (autre user avait tenté et
--     laissé tomber) : perdues. Impact nul en pratique — la slice a un seul
--     claim actif à la fois par design.
--   - `oss_bounties.tags` legacy : sont conservés dans project_slices.external_metadata
--     via mig 0063 (backfill original) + le nouveau flow create_bounty.

DROP TABLE IF EXISTS oss_bounty_claims;
DROP TABLE IF EXISTS oss_bounties;
