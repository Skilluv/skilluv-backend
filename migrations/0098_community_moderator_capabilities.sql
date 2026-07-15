-- Phase P25.1 — Extension du CHECK enum user_capabilities pour modération communautaire.
-- Migration 0098.
--
-- Rationale :
--   Discussion produit : distinguer "power user modérateur" (agit sur
--   skilluv-frontend) de "staff Skilluv admin" (agit sur skilluv-admin).
--
--   La capability `admin` reste réservée au staff Skilluv (accès admin panel
--   système). Les nouvelles caps ci-dessous unlockent la modération inline sur
--   le front user, sans jamais donner accès à l'admin panel :
--
--     - community_moderator   : umbrella meta-cap (auto-granted si une des
--                                sous-caps est active). Sert de check simple.
--     - forum_moderator       : modère threads + posts, mute users 24h.
--     - plagiarism_reviewer   : review deliverables flaggés plagiat, décision
--                                mark_valid/revoke.
--     - kyc_reviewer          : review comptes suspects multi-account +
--                                approve/deny KYC Momo > 100k XOF.
--     - community_curator     : approve/reject les challenges community
--                                proposés par la communauté.
--
--   Ces caps sont **cumulables** avec les autres (P18 base) — un user peut
--   être mentor + pr_reviewer + forum_moderator + community_curator sans
--   conflit. Toutes soumises au même flow revoke/expires_at.

ALTER TABLE user_capabilities
    DROP CONSTRAINT IF EXISTS user_capabilities_capability_check;

ALTER TABLE user_capabilities
    ADD CONSTRAINT user_capabilities_capability_check
    CHECK (capability IN (
        -- P18 base
        'challenger',
        'mentor',
        'project_steward',
        'pr_reviewer',
        'bounty_funder',
        'issue_proposer',
        'jury_tournament',
        'admin',
        'enterprise_recruiter',
        -- P25 : modération communautaire (front user, PAS admin panel)
        'community_moderator',
        'forum_moderator',
        'plagiarism_reviewer',
        'kyc_reviewer',
        'community_curator'
    ));
