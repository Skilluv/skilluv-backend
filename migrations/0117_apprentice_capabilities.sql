-- Phase P26.1 — Extension user_capabilities pour le sas compagnonnage débutant.
-- Migration 0117.
--
-- Rationale :
--   Discussion produit (memory project_ai_policy.md + pédagogie compagnonnage) :
--   le sas d'entrée pour les nouveaux apprentis est un dispositif *humain,
--   asynchrone, ponctuel* (5 challenges initiaux), pas un contrôle technique
--   permanent. Ce qu'on vérifie n'est PAS "l'user maîtrise techniquement",
--   c'est "l'user est bien un humain qui contribue et comprend ce qu'il
--   produit". C'est un passeport d'identité pédagogique, pas un diplôme.
--
--   Deux nouvelles capabilities pour porter ce workflow :
--
--     - verified_apprentice   : accordée automatiquement à l'apprenti après
--                                 N approbations distinctes du sas (default
--                                 3, tunable via SKILLUV_APPRENTICE_SAS_THRESHOLD).
--                                 Débloque les challenges beginner en mode
--                                 libre (sans review humaine).
--     - apprentice_verifier    : capacité de reviewer les vidéos du sas.
--                                 Attribuée manuellement au bootstrap, puis
--                                 auto-granted plus tard sur critères
--                                 d'ancienneté + contributions saines.
--                                 N'est PAS un rôle admin — c'est un
--                                 compagnon volontaire, comme les autres
--                                 caps modération (P25).
--
--   Cumulables avec toutes les autres capabilities. Toutes soumises au flow
--   révoque/expires_at hérité de P18.

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
        'community_curator',
        -- P26 : sas compagnonnage débutant
        'verified_apprentice',
        'apprentice_verifier'
    ));
