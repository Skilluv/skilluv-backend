-- The capabilities stop being a list nobody owns, and become a table.
--
-- ## The warning of 0305, acted on
--
-- Migration 0305 wrote it out: five migrations restate
-- `user_capabilities_capability_check`, a CHECK can only be replaced, and
-- "the sixth will be whichever domain gets review rights next". Audio is the
-- sixth. It could restate thirty-eight values and add six, and leave a
-- seventh migration to make the same bet — or it can stop.
--
-- 0305 added a test as the guard it could afford at the time. A test catches
-- the drop after somebody writes it; a foreign key makes the drop
-- unexpressible. The test stays: it now asserts something that cannot fail,
-- which is the correct end state for a guard around a fixed bug.
--
-- ## The derived half derives itself
--
-- `require_reviewer_for_orientation` builds `{primary_domain}_reviewer:{group}`
-- from a row in `orientations` at request time. So the set of grantable review
-- capabilities is a *function* of the catalogue, and 0305's failure mode was
-- exactly the two drifting apart. A trigger on `orientations` keeps them
-- together: adding a trade with a review family makes the capability
-- grantable in the same statement, and no migration has to remember.
--
-- That is also why audio's six capabilities are not spelled out below. They
-- appear because 0401 seeded five orientations with four review families, and
-- the backfill at the end of this file walks the catalogue. A domain added in
-- 2029 gets them the same way.
--
-- ## `is_derived`
--
-- Says whether the row is owned by the trigger or written by hand. A derived
-- row must not be deleted because somebody archived the last orientation of a
-- family — people already hold the capability, and revoking rights as a side
-- effect of an editorial change is not something a trigger should do. So the
-- trigger only ever inserts, and the flag is what tells an operator reading
-- the table why a row nobody typed is there.

CREATE TABLE capability_catalog (
    capability VARCHAR(48) PRIMARY KEY,
    -- The part before the colon: `admin`, `code_reviewer`, `ai_reviewer`.
    family VARCHAR(32) NOT NULL,
    -- The part after it, NULL when there is none.
    scope VARCHAR(32),
    description TEXT NOT NULL,
    -- TRUE when the row is maintained by the orientations trigger below.
    is_derived BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT capability_catalog_name_matches_its_parts CHECK (
        capability = family || COALESCE(':' || scope, '')
    )
);

COMMENT ON TABLE capability_catalog IS
    'Every capability that can be granted. A table rather than a CHECK because '
    'a CHECK can only be replaced, and migration 0305 documents five rewrites '
    'of the list it replaces — each one an opportunity to silently delete a '
    'value somebody else added.';

INSERT INTO capability_catalog (capability, family, scope, description) VALUES
    -- P18 base (migration 0098)
    ('challenger', 'challenger', NULL,
     'Peut proposer un défi au catalogue.'),
    ('mentor', 'mentor', NULL,
     'Peut accompagner un apprenant et signer un compagnonnage.'),
    ('project_steward', 'project_steward', NULL,
     'Répond des projets d''un terrain : tranches, priorités, accueil des nouveaux.'),
    ('pr_reviewer', 'pr_reviewer', NULL,
     'Peut relire une contribution soumise en amont.'),
    ('bounty_funder', 'bounty_funder', NULL,
     'Peut financer une prime sur une tranche.'),
    ('issue_proposer', 'issue_proposer', NULL,
     'Peut proposer une tranche à partir d''un ticket amont.'),
    ('jury_tournament', 'jury_tournament', NULL,
     'Siège au jury d''un tournoi ou d''un concours.'),
    ('admin', 'admin', NULL,
     'Accès au panneau d''administration.'),
    ('enterprise_recruiter', 'enterprise_recruiter', NULL,
     'Agit au nom d''une entreprise : recherche de talents, missions, offres.'),
    -- P25 community moderation (migration 0176 and before)
    ('community_moderator', 'community_moderator', NULL,
     'Modération générale de la communauté. Englobe les rôles ci-dessous.'),
    ('forum_moderator', 'forum_moderator', NULL,
     'Modération du forum : fils, réponses, signalements.'),
    ('plagiarism_reviewer', 'plagiarism_reviewer', NULL,
     'Instruit les signalements de plagiat.'),
    ('kyc_reviewer', 'kyc_reviewer', NULL,
     'Instruit les vérifications d''identité et de conformité.'),
    ('community_curator', 'community_curator', NULL,
     'Met en avant des travaux et anime les vitrines éditoriales.'),
    -- P26 beginner sas (migration 0117)
    ('verified_apprentice', 'verified_apprentice', NULL,
     'Apprenti dont l''identité et la démarche ont été vérifiées.'),
    ('apprentice_verifier', 'apprentice_verifier', NULL,
     'Peut vérifier un apprenti.'),
    -- Running a domain (migration 0256). Its challenges, its contests, its
    -- featurings — not its people and not its money. Written out rather than
    -- derived from `skill_domains` because `:all` has no domain behind it and
    -- a curator is appointed per domain rather than following the catalogue.
    ('domain_curator:code', 'domain_curator', 'code',
     'Anime le domaine code : ses défis, ses concours, ses mises en avant.'),
    ('domain_curator:design', 'domain_curator', 'design',
     'Anime le domaine design : ses défis, ses concours, ses mises en avant.'),
    ('domain_curator:game', 'domain_curator', 'game',
     'Anime le domaine jeu : ses défis, ses concours, ses mises en avant.'),
    ('domain_curator:security', 'domain_curator', 'security',
     'Anime le domaine cybersécurité : ses défis, ses concours, ses mises en avant.'),
    ('domain_curator:ops', 'domain_curator', 'ops',
     'Anime le domaine infrastructure : ses défis, ses concours, ses mises en avant.'),
    ('domain_curator:ai', 'domain_curator', 'ai',
     'Anime le domaine IA : ses défis, ses concours, ses mises en avant.'),
    ('domain_curator:soft_skills', 'domain_curator', 'soft_skills',
     'Anime le domaine savoir-être : ses défis, ses concours, ses mises en avant.'),
    ('domain_curator:audio', 'domain_curator', 'audio',
     'Anime le domaine audio : ses défis, ses concours, ses mises en avant.'),
    ('domain_curator:all', 'domain_curator', NULL,
     'Anime tous les domaines.');

-- ═══════════════════════════════════════════════════════════════════
-- The derived rows, and the trigger that keeps them
-- ═══════════════════════════════════════════════════════════════════

CREATE FUNCTION capability_catalog_derive_for_orientation(
    _primary_domain VARCHAR,
    _reviewer_group VARCHAR
) RETURNS VOID AS $$
BEGIN
    -- One per domain: the right to validate a challenge in it.
    INSERT INTO capability_catalog (capability, family, scope, description, is_derived)
    VALUES ('challenge_validator:' || _primary_domain,
            'challenge_validator', _primary_domain,
            'Peut valider un défi du domaine ' || _primary_domain || '.',
            TRUE)
    ON CONFLICT (capability) DO NOTHING;

    -- The super-validator of the domain: every family at once.
    INSERT INTO capability_catalog (capability, family, scope, description, is_derived)
    VALUES (_primary_domain || '_reviewer:all',
            _primary_domain || '_reviewer', 'all',
            'Relecture de tous les métiers du domaine ' || _primary_domain || '.',
            TRUE)
    ON CONFLICT (capability) DO NOTHING;

    IF _reviewer_group IS NOT NULL THEN
        INSERT INTO capability_catalog (capability, family, scope, description, is_derived)
        VALUES (_primary_domain || '_reviewer:' || _reviewer_group,
                _primary_domain || '_reviewer', _reviewer_group,
                'Relecture de la famille ' || _reviewer_group || ' du domaine '
                    || _primary_domain || '.',
                TRUE)
        ON CONFLICT (capability) DO NOTHING;
    END IF;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION capability_catalog_derive_for_orientation(VARCHAR, VARCHAR) IS
    'Makes grantable the capabilities `require_reviewer_for_orientation` and '
    '`require_challenge_validator_for` build from an orientation row. Inserts '
    'only: a capability people already hold must not disappear because a trade '
    'was archived.';

CREATE FUNCTION trg_orientations_derive_capabilities() RETURNS TRIGGER AS $$
BEGIN
    PERFORM capability_catalog_derive_for_orientation(
        NEW.primary_domain, NEW.reviewer_group);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_orientations_derive_capabilities
    AFTER INSERT OR UPDATE OF primary_domain, reviewer_group ON orientations
    FOR EACH ROW EXECUTE FUNCTION trg_orientations_derive_capabilities();

-- Every trade the catalogue already holds, including the five audio ones
-- seeded in 0401 — which is how audio gets its six capabilities without this
-- file naming a single one of them.
DO $$
DECLARE
    row RECORD;
BEGIN
    FOR row IN SELECT DISTINCT primary_domain, reviewer_group FROM orientations LOOP
        PERFORM capability_catalog_derive_for_orientation(
            row.primary_domain, row.reviewer_group);
    END LOOP;
END $$;

-- The domains that carry no orientation yet still validate challenges. Every
-- active domain gets its validator capability, whether or not a trade has been
-- written for it: the review queue is keyed by domain, not by trade.
INSERT INTO capability_catalog (capability, family, scope, description, is_derived)
SELECT 'challenge_validator:' || slug, 'challenge_validator', slug,
       'Peut valider un défi du domaine ' || slug || '.', TRUE
  FROM skill_domains WHERE is_active
ON CONFLICT (capability) DO NOTHING;

-- Anything granted before this table existed but not derivable from the
-- catalogue. Nothing is expected — the list came from the CHECK — and the
-- insert is here so that a database with a hand-granted capability migrates
-- instead of failing on the foreign key below.
INSERT INTO capability_catalog (capability, family, scope, description)
SELECT DISTINCT uc.capability,
       split_part(uc.capability, ':', 1),
       NULLIF(split_part(uc.capability, ':', 2), ''),
       'Reprise : accordée avant que le catalogue existe.'
  FROM user_capabilities uc
 WHERE NOT EXISTS (
        SELECT 1 FROM capability_catalog c WHERE c.capability = uc.capability)
ON CONFLICT (capability) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- The CHECK becomes a foreign key
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE user_capabilities
    DROP CONSTRAINT IF EXISTS user_capabilities_capability_check,
    ADD CONSTRAINT user_capabilities_capability_fkey
        FOREIGN KEY (capability) REFERENCES capability_catalog(capability)
        ON UPDATE CASCADE;

COMMENT ON CONSTRAINT user_capabilities_capability_fkey ON user_capabilities IS
    'Points at `capability_catalog`. Replaces the CHECK that five migrations '
    'restated and that 0305 predicted a sixth would too: a new capability is '
    'now an INSERT, which cannot delete anybody else''s.';
