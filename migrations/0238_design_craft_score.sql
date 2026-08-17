-- The craft score, for design.
--
-- Migration 0195 built the formula as rows and 0204 made the storage
-- per-domain, so this is a seed rather than a mechanism. Nothing new is
-- invented; design states what it counts and what each thing is worth.
--
-- ## What design counts that code does not
--
-- Two terms carry most of the difference.
--
-- `iterations_converged` rewards deliverables that took three critique rounds
-- or more and still reached validation. Nothing in code has an equivalent
-- because a pull request that needed four passes is not usually a better pull
-- request. In design it usually is a better designer: being told the
-- direction is wrong and coming back is the harder thing, and a score that
-- only counted first-round approvals would quietly favour the timid brief.
--
-- `trades_distinct` counts how many of the twenty-six trades somebody has
-- been validated in. The design domain is unusually wide — a type designer
-- and a service designer share almost no craft — so range is worth saying out
-- loud, and it is the one thing volume cannot buy.
--
-- ## What is deliberately absent
--
-- Anything imported. No Behance project count, no Dribbble follower number.
-- Migration 0145 keeps external signals display-only, and a score that could
-- be moved by importing a portfolio would stop meaning "proven here".
--
-- ## Why the weights sit where they do
--
-- A validated deliverable is 20 against code's 15 for a merged pull request:
-- a design challenge carries a critique conversation, which is more work on
-- both sides. A contest win is 150 because winning is rare and public. Taking
-- part is 10 — worth saying, not worth much, which is the honest ratio.

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order)
VALUES
    ('design', 'deliverables_validated', 20, 'count', NULL,
     'Livrables design validés après critique', 10),

    ('design', 'iterations_converged', 35, 'count', NULL,
     'Livrables menés à la validation après trois tours de critique ou plus', 20),

    ('design', 'review_grid_average', 200, 'offset_scaled', 3.0,
     'Moyenne des grilles de revue reçues, au-dessus de la moyenne', 30),

    ('design', 'trades_distinct', 60, 'count', NULL,
     'Métiers design différents dans lesquels le travail a été validé', 40),

    ('design', 'contests_won', 150, 'count', NULL,
     'Concours design remportés', 50),

    ('design', 'contests_entered', 10, 'count', NULL,
     'Concours design auxquels le travail a été présenté', 60),

    ('design', 'jury_service', 40, 'count', NULL,
     'Concours jugés en tant que membre du jury', 70),

    ('design', 'brand_systems_delivered', 120, 'count', NULL,
     'Identités de marque complètes livrées avec leurs guidelines', 80),

    ('design', 'typefaces_released', 200, 'count', NULL,
     'Familles de caractères publiées avec leurs fichiers de production', 90),

    ('design', 'systems_adopted', 180, 'count', NULL,
     'Design systems sur lesquels une autre équipe construit', 100),

    ('design', 'missions_completed', 100, 'count', NULL,
     'Missions design payées, acceptées par le client', 110),

    ('design', 'years_active', 25, 'count', NULL,
     'Années entières depuis le premier artefact vérifié', 120),

    ('design', 'featured_times', 200, 'count', NULL,
     'Fois où le travail a été mis en avant par la plateforme', 130)
ON CONFLICT (skill_domain, term) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- The words next to somebody's name
-- ═══════════════════════════════════════════════════════════════════
--
-- Not the code ladder with the nouns swapped. "Staff designer" is a title
-- that exists in three companies and confuses everybody else, while the words
-- below say what the person can be asked to do — which is what a recruiter
-- and a beginner are both actually reading for.

INSERT INTO craft_score_tiers
    (skill_domain, slug, name, min_score, max_score, description, sort_order)
VALUES
    ('design', 'apprentice', 'Apprenti', 0, 99,
     'Les premiers livrables. Le seul palier que tout le monde traverse.', 1),
    ('design', 'praticien', 'Praticien', 100, 499,
     'Des livrables réguliers, critiqués par d''autres et repris.', 2),
    ('design', 'artisan', 'Artisan', 500, 1499,
     'Du travail livré de bout en bout, sur plusieurs briefs et plusieurs supports.', 3),
    ('design', 'auteur', 'Auteur', 1500, 3499,
     'Une direction reconnaissable, tenue d''un projet à l''autre.', 4),
    ('design', 'referent', 'Référent', 3500, 6999,
     'Un travail sur lequel d''autres s''appuient : systèmes, guidelines, critiques.', 5),
    ('design', 'maitre', 'Maître d''œuvre', 7000, NULL,
     'Une contribution qui a changé la façon de faire de toute une communauté.', 6)
ON CONFLICT (skill_domain, slug) DO NOTHING;
