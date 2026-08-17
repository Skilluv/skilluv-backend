-- The AI craft score: a formula in rows, and the words that go next to it.
--
-- ## Why nothing new is created here
--
-- Migration 0195 built `craft_score_weights` and `craft_score_tiers` keyed by
-- domain, and 0204 made the storage match. Everything AI needs is rows in
-- those three tables. The only thing that cannot be a row is the measuring,
-- because each term counts a different table — which is why there is a second
-- service rather than a branch in the first, as 0195 said there would be.
--
-- ## The ratios, and what they say
--
-- A paper is worth a hundred and a verified artefact five. That is a
-- statement about what this domain values, not a measurement, and the point
-- of putting it in a table is that somebody can argue with it in the admin
-- panel instead of waiting for a deployment.
--
-- Two of them deserve their reasoning out loud:
--
--   * **a reproduced benchmark is worth more than a paper.** An unverified
--     SOTA claim is the single easiest thing to overstate in this domain, and
--     one that a stranger has re-run and confirmed is the hardest. The weight
--     follows the difficulty, not the prestige.
--   * **downloads are logarithmic and modest.** Reach depends on the subject
--     as much as on the craft: one model that goes around the world should be
--     visible in the score and must not outweigh a career.
--
-- ## The ceiling
--
-- Six thousand rather than the code domain's ten. Not because the work is
-- worth less — because there are fewer ways to leave a trace in it, and a cap
-- that nobody can approach is a cap that says nothing.

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order)
VALUES
    ('ai', 'attestations_ai', 5, 'count', NULL,
     'Chaque attestation IA délivrée.', 10),
    ('ai', 'models_shipped', 60, 'count', NULL,
     'Chaque modèle publié à une adresse où un inconnu peut l''exécuter.', 20),
    ('ai', 'datasets_published', 40, 'count', NULL,
     'Chaque jeu de données publié avec sa fiche : provenance, licence, limites.', 30),
    ('ai', 'agent_systems_deployed', 50, 'count', NULL,
     'Chaque système d''agents en service, évaluations et garde-fous compris.', 40),
    ('ai', 'papers_published', 100, 'count', NULL,
     'Chaque article paru, avec le code qui le soutient.', 50),
    ('ai', 'benchmarks_reproduced', 150, 'count', NULL,
     'Chaque résultat de banc qu''un tiers a rejoué et retrouvé. Vaut plus '
     'qu''un article : une annonce de record est ce qu''on surestime le plus '
     'facilement, et une reproduction ce qu''on obtient le plus difficilement.', 60),
    ('ai', 'safety_findings_validated', 80, 'count', NULL,
     'Chaque trouvaille de sûreté reproduite et divulguée dans les règles.', 70),
    ('ai', 'missions_completed', 100, 'count', NULL,
     'Chaque mission IA payée menée à son terme.', 80),
    ('ai', 'hub_downloads', 40, 'log_scaled', NULL,
     'Les téléchargements mensuels cumulés sur HuggingFace et Kaggle, sur une '
     'échelle logarithmique : un million vaut environ le double de mille. '
     'L''audience dépend du sujet autant que du métier.', 90),
    ('ai', 'review_grid_average', 200, 'offset_scaled', 3.0,
     'La moyenne des grilles de relecture, comptée à partir de 3 sur 5 : le '
     'milieu de la grille ne vaut rien, ce qui compte est l''écart.', 100),
    ('ai', 'orientations_distinct', 20, 'count', NULL,
     'Chaque métier IA dans lequel un artefact vérifié existe.', 110),
    ('ai', 'years_active', 25, 'count', NULL,
     'Chaque année depuis le premier artefact IA vérifié.', 120),
    ('ai', 'featured_times', 200, 'count', NULL,
     'Chaque mise en avant éditoriale.', 130);

-- ═══════════════════════════════════════════════════════════════════
-- The tiers
-- ═══════════════════════════════════════════════════════════════════
--
-- The names the trade actually uses. `researcher` sits above `senior` here
-- and would not in the code domain: in AI it names a distinct path rather
-- than a rung above engineering, and pretending otherwise would tell a senior
-- MLOps engineer they are below somebody who publishes.
--
-- It is placed high anyway, and the reason is the same as the benchmark
-- weight: what it takes to get there is published work a stranger can check.

INSERT INTO craft_score_tiers
    (skill_domain, slug, name, min_score, max_score, description, sort_order)
VALUES
    ('ai', 'apprentice', 'Apprenti', 0, 99,
     'Les premiers artefacts. Le seul palier que tout le monde traverse.', 1),
    ('ai', 'practitioner', 'Praticien', 100, 499,
     'Des modèles entraînés et évalués honnêtement, régulièrement.', 2),
    ('ai', 'engineer', 'Ingénieur', 500, 1499,
     'Du travail mis en service et surveillé, pas seulement entraîné.', 3),
    ('ai', 'senior', 'Senior', 1500, 3499,
     'Une trace assez large pour que d''autres s''appuient dessus.', 4),
    ('ai', 'researcher', 'Chercheur', 3500, 5499,
     'Des résultats publiés qu''un tiers a pu rejouer.', 5),
    ('ai', 'principal', 'Principal', 5500, NULL,
     'Une contribution qui a changé quelque chose pour un champ entier.', 6);
