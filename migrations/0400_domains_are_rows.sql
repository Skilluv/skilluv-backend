-- The domains stop being a list nobody owns, and become a table.
--
-- ## The failure this ends
--
-- Ten CHECK constraints spell the seven domains out, one per table, and a
-- CHECK cannot be extended — only replaced. Migration 0228 wrote down what
-- that costs when it happened to `tournaments_kind_check`: 0223 restated the
-- list, dropped what 0189 had added, and nothing failed until somebody tried
-- to create a contest. Migration 0305 wrote the same warning on
-- `user_capabilities_capability_check`, where five migrations have now
-- restated the same list.
--
-- The domain list is the one this happens to most, because every domain that
-- arrives touches all ten:
--
--   * 0002 and 0003 wrote four;
--   * 0056 and 0088 gave `skill_nodes` and `orientations` seven;
--   * 0219 discovered — a year later, by trying to insert one — that three of
--     those seven could never carry a challenge, a user or a sponsored
--     request, and widened three more tables by hand;
--   * ten further tables have been added since holding a domain with nothing
--     checking it at all.
--
-- Migration 0204 says out loud that eleven domains are documented, and seeds
-- `craft_score_tiers` for all of them. So the platform already believes in a
-- list it can only enforce in fragments, and which fragment you happen to
-- touch decides whether `audio` is a domain or a typo.
--
-- ## What replaces it
--
-- One table, and a foreign key from every column that holds a domain. Adding
-- the twelfth becomes an INSERT, which cannot silently delete the eleventh:
-- the failure mode goes away rather than being documented a third time.
--
-- ## Why the ten unconstrained columns get one too
--
-- `missions`, `content_guides`, `review_grids`, `certifications` and six
-- others hold a domain with nothing checking it. That is not freedom, it is
-- the same bug with the alarm switched off: a mission created with
-- `skill_domain = 'Audio'` is invisible to every query looking for `audio`,
-- and nothing reports it. They are constrained here for the first time.
--
-- ## Declared is not the same as open
--
-- Twelve rows go in and seven are active. The other five are the ones 0204
-- already seeded tiers for: they have to exist for a foreign key to point at
-- them, and `is_active` is what a listing reads before offering a domain to
-- somebody choosing. A domain with no orientations, no challenges and no
-- review grid is not one anybody should be able to pick, and the flag says so
-- without the row having to be absent.

CREATE TABLE skill_domains (
    slug VARCHAR(30) PRIMARY KEY,
    name VARCHAR(60) NOT NULL,
    description TEXT NOT NULL,
    -- The reading categories of migration 0091. Held here rather than in a
    -- CASE inside `skill_nodes_default_display_category`, which is the
    -- eleventh copy of the domain list and the one that fails silently: an
    -- unknown domain falls through to 'craft' rather than raising.
    display_category VARCHAR(20) NOT NULL
        CHECK (display_category IN ('craft', 'create', 'understand', 'operate', 'share')),
    -- Whether somebody can choose it. A domain with no catalogue behind it is
    -- vocabulary, not an offer.
    is_active BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order SMALLINT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE skill_domains IS
    'Every domain the platform knows, as rows. Referenced by foreign key from '
    'every column that holds one, so a new domain is an INSERT rather than ten '
    'CHECK constraints restated — the rewrite migrations 0228 and 0305 '
    'document losing values to.';

COMMENT ON COLUMN skill_domains.is_active IS
    'Whether the domain can be chosen. Rows exist before they are open: a '
    'foreign key needs the row, and a person choosing needs a catalogue behind '
    'it. Read by the listings, never by the constraints.';

INSERT INTO skill_domains (slug, name, description, display_category, is_active, sort_order) VALUES
    ('code', 'Code',
     'Écrire, faire tenir et faire évoluer du logiciel.',
     'craft', TRUE, 10),
    ('design', 'Design',
     'Interfaces, identités, systèmes de design. Ce qui se voit et ce qui se traverse.',
     'create', TRUE, 20),
    ('game', 'Jeu vidéo',
     'Moteurs, mécaniques, niveaux, assets. Faire un jeu, pas seulement un logiciel.',
     'create', TRUE, 30),
    ('security', 'Cybersécurité',
     'Attaquer selon un protocole écrit, défendre avec des preuves.',
     'operate', TRUE, 40),
    ('ops', 'Infrastructure et exploitation',
     'Servir, surveiller, redéployer. Ce qui tourne quand personne ne regarde.',
     'operate', TRUE, 50),
    ('ai', 'Intelligence artificielle',
     'Données, modèles, agents, sûreté. Ce qui apprend, et ce qu''il en coûte de le vérifier.',
     'understand', TRUE, 60),
    ('soft_skills', 'Savoir-être et transmission',
     'Relecture, écriture, mentorat. Le métier qui se pratique sur celui des autres.',
     'share', TRUE, 70),
    -- Declared by migration 0204, which seeded their tiers. No catalogue yet.
    ('audio', 'Audio',
     'Composition, sound design, voix, implémentation. Les métiers du son.',
     'create', FALSE, 80),
    ('quality', 'Qualité et test',
     'Stratégie de test, automatisation, non-régression.',
     'operate', FALSE, 90),
    ('leadership', 'Conduite technique',
     'Décider, arbitrer, tenir un cap technique avec d''autres.',
     'share', FALSE, 100),
    ('communication', 'Communication et contenu',
     'Documentation, vulgarisation, prise de parole technique.',
     'share', FALSE, 110),
    ('education', 'Pédagogie',
     'Concevoir un parcours, enseigner, évaluer.',
     'share', FALSE, 120);

CREATE TRIGGER trg_skill_domains_updated_at
    BEFORE UPDATE ON skill_domains
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The ten CHECKs become foreign keys
-- ═══════════════════════════════════════════════════════════════════
--
-- Same values, same refusals. The difference is that the next domain does not
-- have to be written into any of them.

ALTER TABLE challenge_templates
    DROP CONSTRAINT IF EXISTS challenge_templates_skill_domain_check,
    DROP CONSTRAINT IF EXISTS challenges_skill_domain_check,
    ADD CONSTRAINT challenge_templates_skill_domain_fkey
        FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE orientations
    DROP CONSTRAINT IF EXISTS orientations_primary_domain_check,
    ADD CONSTRAINT orientations_primary_domain_fkey
        FOREIGN KEY (primary_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE project_slices
    DROP CONSTRAINT IF EXISTS project_slices_primary_domain_check,
    ADD CONSTRAINT project_slices_primary_domain_fkey
        FOREIGN KEY (primary_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE review_tasks
    DROP CONSTRAINT IF EXISTS review_tasks_primary_domain_check,
    ADD CONSTRAINT review_tasks_primary_domain_fkey
        FOREIGN KEY (primary_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE skill_nodes
    DROP CONSTRAINT IF EXISTS skill_nodes_domain_check,
    ADD CONSTRAINT skill_nodes_domain_fkey
        FOREIGN KEY (domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE sponsored_challenge_requests
    DROP CONSTRAINT IF EXISTS sponsored_challenge_requests_skill_domain_check,
    ADD CONSTRAINT sponsored_challenge_requests_skill_domain_fkey
        FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE tracks
    DROP CONSTRAINT IF EXISTS tracks_target_domain_check,
    ADD CONSTRAINT tracks_target_domain_fkey
        FOREIGN KEY (target_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE user_domain_profiles
    DROP CONSTRAINT IF EXISTS user_domain_profiles_domain_check,
    ADD CONSTRAINT user_domain_profiles_domain_fkey
        FOREIGN KEY (domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE users
    DROP CONSTRAINT IF EXISTS users_skill_domain_check,
    ADD CONSTRAINT users_skill_domain_fkey
        FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE validator_applications
    DROP CONSTRAINT IF EXISTS validator_applications_domain_check,
    ADD CONSTRAINT validator_applications_domain_fkey
        FOREIGN KEY (domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

-- ═══════════════════════════════════════════════════════════════════
-- The ten that had nothing
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE certifications
    ADD CONSTRAINT certifications_skill_domain_fkey
        FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE content_guides
    ADD CONSTRAINT content_guides_skill_domain_fkey
        FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE craft_score_tiers
    ADD CONSTRAINT craft_score_tiers_skill_domain_fkey
        FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE craft_score_weights
    ADD CONSTRAINT craft_score_weights_skill_domain_fkey
        FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE craft_scores
    ADD CONSTRAINT craft_scores_skill_domain_fkey
        FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE mission_types
    ADD CONSTRAINT mission_types_skill_domain_fkey
        FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE missions
    ADD CONSTRAINT missions_skill_domain_fkey
        FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE recruitment_campaigns
    ADD CONSTRAINT recruitment_campaigns_target_domain_fkey
        FOREIGN KEY (target_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE reverse_recruitment_postings
    ADD CONSTRAINT reverse_recruitment_postings_desired_domain_fkey
        FOREIGN KEY (desired_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE review_grids
    ADD CONSTRAINT review_grids_domain_fkey
        FOREIGN KEY (domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_skill_domain_fkey
        FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

-- ═══════════════════════════════════════════════════════════════════
-- The eleventh copy of the list, in a function that failed silently
-- ═══════════════════════════════════════════════════════════════════
--
-- Migration 0116 derives a skill's reading category from its domain with a
-- CASE and an `ELSE 'craft'`. A domain missing from that CASE does not raise;
-- it files every skill of that domain under the wrong heading, and the only
-- reason anybody noticed last time is that a test asserted one specific
-- mapping. The mapping now lives on the row it describes.

CREATE OR REPLACE FUNCTION skill_nodes_default_display_category(_domain VARCHAR)
RETURNS VARCHAR AS $$
    SELECT display_category FROM skill_domains WHERE slug = _domain;
$$ LANGUAGE sql STABLE;

COMMENT ON FUNCTION skill_nodes_default_display_category(VARCHAR) IS
    'The reading category of a domain, read from `skill_domains`. NULL for a '
    'domain that does not exist — the foreign key is what refuses that row, '
    'and this function has no business guessing on its behalf.';

-- The trigger of 0116 compared against a value that can now be NULL. Rewritten
-- so an unknown domain leaves the category alone instead of comparing NULL and
-- silently keeping 'craft'.
CREATE OR REPLACE FUNCTION skill_nodes_set_display_category()
RETURNS TRIGGER AS $$
DECLARE
    derived VARCHAR;
BEGIN
    derived := skill_nodes_default_display_category(NEW.domain);
    IF NEW.display_category = 'craft'
       AND derived IS NOT NULL
       AND derived <> 'craft' THEN
        NEW.display_category := derived;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
