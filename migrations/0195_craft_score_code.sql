-- The code craft score.
--
-- ## What it is for
--
-- One number, so that a profile can be sorted, filtered and compared without
-- a recruiter having to read thirty artefacts to form an impression. It is a
-- summary of what is already on the profile, never a substitute for it: every
-- term below is a count of something with a link behind it.
--
-- ## Why the weights live in a table
--
-- The formula is a judgement about what the platform values, and it will be
-- wrong the first time. A contribution to a web standard is currently worth
-- two hundred points and a merged pull request fifteen — that ratio is a
-- statement, and somebody should be able to argue with it without waiting for
-- a deployment.
--
-- Rows also mean the formula can be read. A score nobody can explain is a
-- score nobody trusts, and "we ran a query" is not an explanation.
--
-- ## Why the score is stored and not computed on read
--
-- Fourteen counts across nine tables, per profile, on every listing. Stored,
-- recomputed on a schedule and after anything that could move it.

ALTER TABLE users
    ADD COLUMN craft_score_code INTEGER NOT NULL DEFAULT 0
        CHECK (craft_score_code >= 0 AND craft_score_code <= 10000),
    ADD COLUMN craft_score_code_computed_at TIMESTAMPTZ;

COMMENT ON COLUMN users.craft_score_code IS
    'Summary of what is already on the profile, capped at 10000. Never a '
    'substitute for the artefacts: every term is a count of something with a '
    'link behind it.';

CREATE INDEX idx_users_craft_score_code
    ON users (craft_score_code DESC)
    WHERE craft_score_code > 0;

-- ═══════════════════════════════════════════════════════════════════
-- The formula, as rows
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE craft_score_weights (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    skill_domain VARCHAR(30) NOT NULL,
    -- What is being counted. The service knows how to count each of these;
    -- an unknown term is ignored rather than guessed at, and logged.
    term VARCHAR(60) NOT NULL,
    -- Points per unit for a count, or the multiplier for a scaled term.
    weight NUMERIC(8,2) NOT NULL,
    -- `count` multiplies the weight by the number of things.
    -- `log_scaled` multiplies by log10(1 + n), so a million downloads is
    --   worth about twice a thousand rather than a thousand times.
    -- `offset_scaled` multiplies by (value - baseline), for the review
    --   average: a grid average of 3 is the middle and worth nothing.
    kind VARCHAR(20) NOT NULL DEFAULT 'count'
        CHECK (kind IN ('count', 'log_scaled', 'offset_scaled')),
    baseline NUMERIC(8,2),
    -- Written for a human. This is what the profile shows next to the number.
    explanation TEXT NOT NULL,
    sort_order SMALLINT NOT NULL DEFAULT 100,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (skill_domain, term),

    -- `offset_scaled` without a baseline is a term that would credit the full
    -- value of a neutral score.
    CONSTRAINT offset_terms_state_their_baseline
        CHECK (kind <> 'offset_scaled' OR baseline IS NOT NULL)
);

COMMENT ON TABLE craft_score_weights IS
    'The craft score formula, as rows. A ratio between two weights is a '
    'statement about what the platform values, and somebody should be able to '
    'argue with it without waiting for a deployment.';

CREATE TRIGGER trg_craft_score_weights_updated_at
    BEFORE UPDATE ON craft_score_weights
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The tiers
-- ═══════════════════════════════════════════════════════════════════
--
-- Also rows, and for a stronger reason than the weights: these are the words
-- that appear next to somebody's name. Changing "Engineer" to something else,
-- or moving where "Senior" starts, must not be a code change.

CREATE TABLE craft_score_tiers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    skill_domain VARCHAR(30) NOT NULL,
    slug VARCHAR(40) NOT NULL,
    name VARCHAR(60) NOT NULL,
    -- Inclusive. The highest tier's `max_score` is NULL: there is no ceiling
    -- above Principal other than the cap itself.
    min_score INTEGER NOT NULL CHECK (min_score >= 0),
    max_score INTEGER,
    description TEXT NOT NULL,
    sort_order SMALLINT NOT NULL,

    UNIQUE (skill_domain, slug),
    CONSTRAINT tier_range_runs_forward
        CHECK (max_score IS NULL OR max_score > min_score)
);

COMMENT ON TABLE craft_score_tiers IS
    'The words that appear next to somebody''s name. Rows, because moving '
    'where "Senior" starts must not be a code change.';

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order)
VALUES
    ('code', 'attestations_code', 5, 'count', NULL,
     'Chaque attestation code délivrée.', 10),
    ('code', 'prs_merged_upstream', 15, 'count', NULL,
     'Chaque pull request fusionnée dans un projet tiers.', 20),
    ('code', 'projects_shipped', 40, 'count', NULL,
     'Chaque projet livré et accessible.', 30),
    ('code', 'libraries_published', 30, 'count', NULL,
     'Chaque bibliothèque publiée sur un registre.', 40),
    ('code', 'library_downloads', 50, 'log_scaled', NULL,
     'Les téléchargements cumulés, sur une échelle logarithmique : un million '
     'vaut environ le double de mille, pas mille fois plus.', 50),
    ('code', 'rfcs_accepted', 80, 'count', NULL,
     'Chaque RFC acceptée par sa communauté.', 60),
    ('code', 'standard_contributions', 200, 'count', NULL,
     'Chaque contribution retenue dans un standard (TC39, IETF, W3C). La '
     'contribution technique la plus durable qui soit.', 70),
    ('code', 'devtools_adopted', 60, 'count', NULL,
     'Chaque outil de développement repris par d''autres.', 80),
    ('code', 'missions_completed', 100, 'count', NULL,
     'Chaque mission payée menée à son terme.', 90),
    ('code', 'review_grid_average', 200, 'offset_scaled', 3.0,
     'La moyenne des grilles de relecture, comptée à partir de 3 sur 5 : le '
     'milieu de la grille ne vaut rien, ce qui compte est l''écart.', 100),
    ('code', 'languages_distinct', 20, 'count', NULL,
     'Chaque langage dans lequel un artefact vérifié existe.', 110),
    ('code', 'years_active', 25, 'count', NULL,
     'Chaque année depuis le premier artefact vérifié.', 120),
    ('code', 'featured_times', 200, 'count', NULL,
     'Chaque mise en avant éditoriale.', 130);

INSERT INTO craft_score_tiers
    (skill_domain, slug, name, min_score, max_score, description, sort_order)
VALUES
    ('code', 'apprentice', 'Apprentice', 0, 99,
     'Les premiers artefacts. Le seul palier que tout le monde traverse.', 1),
    ('code', 'contributor', 'Contributor', 100, 499,
     'Des contributions régulières, relues par d''autres.', 2),
    ('code', 'engineer', 'Engineer', 500, 1499,
     'Du travail livré de bout en bout, sur plusieurs projets.', 3),
    ('code', 'senior', 'Senior', 1500, 3499,
     'Une trace suffisamment large pour que d''autres s''appuient dessus.', 4),
    ('code', 'staff', 'Staff', 3500, 6999,
     'Un impact au-delà de son propre code : outils, bibliothèques, relectures.', 5),
    ('code', 'principal', 'Principal', 7000, NULL,
     'Une contribution qui a changé quelque chose pour une communauté entière.', 6);
