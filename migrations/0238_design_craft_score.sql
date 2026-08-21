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
-- No tiers here, on purpose
-- ═══════════════════════════════════════════════════════════════════
--
-- Migration 0204 already gave design the six tiers of the code ladder, and
-- said why: a tier is a position on a scale, each scale is calibrated by its
-- own weights, and a vocabulary per domain would stop anybody comparing a
-- profile to itself across two domains.
--
-- An earlier draft of this migration seeded a French design ladder —
-- Praticien, Artisan, Auteur, Référent — beside them. Every score band ended
-- up covered by two rows with the same `min_score`, and the tier lookup
-- (`ORDER BY min_score DESC LIMIT 1`) would have returned whichever the
-- planner reached first. A designer's tier would have been Engineer or
-- Artisan depending on the day.
--
-- The weights above are the whole of design's contribution, which is what
-- 0204 said it would be.
