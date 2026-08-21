-- Fifteen code distinctions, and an honest way to award the ones no rule
-- can decide.
--
-- ## Two kinds of badge
--
-- Nine are derived: a count of verified deliverables, of slices merged
-- upstream, of distinct languages. The engine awards them and revokes them
-- when the proof behind them is revoked, which is what makes them worth
-- something.
--
-- Six are judgements. "Shipped an audited contract to mainnet" and
-- "contributed to a standard" are not row counts, and a rule invented to
-- approximate them would award them to the wrong people. They are marked
-- `manual`: the engine never touches them, an operator grants them, and the
-- reason is recorded next to the grant.
--
-- The alternative was to leave them out. That would be worse: the
-- distinction exists in the trade whether or not our schema can compute it,
-- and a platform that only recognises what it can count ends up measuring
-- what is easy instead of what matters.

-- ═══════════════════════════════════════════════════════════════════
-- One badge per person, per badge — not per person
-- ═══════════════════════════════════════════════════════════════════
--
-- The primary key was (user_id, badge_id). Every rule the proof engine
-- derives points at the same sentinel row in `badges`, because rules live in
-- `badge_rules` and the legacy foreign key still had to be satisfied. So the
-- second derived badge a person earned collided with the first, and the
-- recompute failed: the engine could award exactly one badge per user, ever.
--
-- Identity differs between the two systems. A legacy badge is identified by
-- `badge_id`; a derived one by `rule_id`, the sentinel being an artefact of
-- the foreign key rather than the thing awarded. Two partial uniques say
-- that, where one composite key could not.

ALTER TABLE user_badges
    DROP CONSTRAINT user_badges_pkey;

ALTER TABLE user_badges
    ADD COLUMN id UUID NOT NULL DEFAULT gen_random_uuid();

ALTER TABLE user_badges
    ADD CONSTRAINT user_badges_pkey PRIMARY KEY (id);

CREATE UNIQUE INDEX uniq_user_badges_legacy
    ON user_badges (user_id, badge_id)
    WHERE rule_id IS NULL;

CREATE UNIQUE INDEX uniq_user_badges_by_rule
    ON user_badges (user_id, rule_id)
    WHERE rule_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Who granted a badge nobody computed
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE user_badges
    ADD COLUMN granted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN grant_reason TEXT;

COMMENT ON COLUMN user_badges.granted_by IS
    'The operator who awarded a manual badge. NULL when the engine derived '
    'it from proofs, which is the normal case.';

COMMENT ON COLUMN user_badges.grant_reason IS
    'Why a manual badge was awarded. Required for manual grants: a '
    'distinction handed out with no stated reason is indistinguishable from '
    'favouritism, and this platform sells the opposite.';

-- A manual grant carries both or neither. Half of it is worse than none:
-- a reason with no author cannot be questioned, an author with no reason
-- cannot be explained.
ALTER TABLE user_badges
    ADD CONSTRAINT user_badges_manual_grant_is_complete
    -- `grant_reason IS NOT NULL` before the trim, and not only for tidiness:
    -- `btrim(NULL) <> ''` is NULL, a CHECK that evaluates to NULL passes, and
    -- the constraint would have accepted the exact case it exists to refuse.
    CHECK (
        (granted_by IS NULL AND grant_reason IS NULL)
        OR (granted_by IS NOT NULL
            AND grant_reason IS NOT NULL
            AND btrim(grant_reason) <> '')
    );

CREATE INDEX idx_user_badges_manual
    ON user_badges (granted_by)
    WHERE granted_by IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- The nine the engine decides
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('code-first-artifact', 'medal',
 'Premier artefact',
 'Un premier livrable vérifié. Le moment où le profil cesse d''être déclaratif.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "code", "min_count": 1}', 'common'),

('code-craft-master', 'medal',
 'Maître d''œuvre',
 'Trente livrables vérifiés. La régularité, pas le coup d''éclat.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "code", "min_count": 30}', 'epic'),

('code-craft-legend', 'medal',
 'Légende de l''atelier',
 'Cent livrables vérifiés.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "code", "min_count": 100}', 'legendary'),

('code-oss-contributor', 'medal',
 'Contributeur open source',
 'Cinq contributions mergées en amont. Du code que quelqu''un d''autre maintient désormais.',
 '{"proof_types": ["slice_merged_upstream"], "skill_domain": "code", "min_count": 5}', 'rare'),

('code-oss-veteran', 'medal',
 'Vétéran open source',
 'Trente contributions mergées en amont.',
 '{"proof_types": ["slice_merged_upstream"], "skill_domain": "code", "min_count": 30}', 'epic'),

('code-oss-legend', 'medal',
 'Légende open source',
 'Cent contributions mergées en amont.',
 '{"proof_types": ["slice_merged_upstream"], "skill_domain": "code", "min_count": 100}', 'legendary'),

('code-multi-language', 'medal',
 'Polyglotte',
 'Des livrables vérifiés dans trois langages différents. Le métier, pas la syntaxe.',
 '{"distinct_over": "challenge_language", "skill_domain": "code", "min_count": 3}', 'rare'),

('code-featured', 'medal',
 'Mis en avant',
 'Un livrable retenu par la rédaction pour son exemplarité.',
 '{"proof_types": ["deliverable_featured"], "skill_domain": "code", "min_count": 1}', 'rare'),

('code-first-upstream-merge', 'medal',
 'Première fusion en amont',
 'Une première contribution acceptée dans un dépôt qu''on ne contrôle pas.',
 '{"proof_types": ["slice_merged_upstream"], "skill_domain": "code", "min_count": 1}', 'rare');

-- ═══════════════════════════════════════════════════════════════════
-- The six an operator decides
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('code-multi-domain', 'medal',
 'Touche-à-tout',
 'Du travail vérifié dans trois orientations code différentes.',
 -- Manual until a deliverable records which orientation it belongs to.
 -- The link does not exist: a deliverable points at a challenge, and a
 -- challenge carries a domain, not a trade.
 '{"manual": true}', 'epic'),

('code-web-fullstack-mastery', 'medal',
 'Maîtrise fullstack',
 'Un produit web mené seul de la base de données à l''interface, et mis en service.',
 '{"manual": true}', 'epic'),

('code-systems-hero', 'medal',
 'Bas niveau',
 'Une contribution substantielle au noyau, à un pilote ou à du firmware.',
 '{"manual": true}', 'legendary'),

('code-blockchain-shipper', 'medal',
 'Déployé en production on-chain',
 'Un contrat audité puis déployé sur un réseau principal.',
 '{"manual": true}', 'legendary'),

('code-devtool-author', 'medal',
 'Auteur d''outil',
 'Un outil de développement utilisé par d''autres — pas seulement publié.',
 '{"manual": true}', 'epic'),

('code-standards-contributor', 'medal',
 'Contributeur aux standards',
 'Une contribution retenue dans une RFC ou une spécification ouverte.',
 '{"manual": true}', 'legendary');
