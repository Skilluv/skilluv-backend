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
-- The same ten thousand as code. An earlier draft used six, on the grounds
-- that there are fewer ways to leave a trace in this domain — which would
-- have put the top tier, starting at seven thousand, out of reach forever.
-- A ceiling below a threshold is a tier nobody can hold.

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
-- No tiers here
-- ═══════════════════════════════════════════════════════════════════
--
-- Migration 0204 already gave every domain the same six, and states why: a
-- tier is a position on a scale, the scales are each calibrated by their own
-- weights, and giving each domain its own vocabulary would mean nobody can
-- compare a profile to itself across two domains.
--
-- That argument holds, and an earlier draft of this migration broke it —
-- inventing `practitioner` and `researcher` for AI, with French names and
-- lower thresholds. It would have meant "Chercheur at 3500" facing "Staff at
-- 3500" on the same person's profile, with no way to tell whether that was
-- the same distance travelled.
--
-- So the vocabulary is shared and the calibration is above: the AI weights
-- are lower in aggregate than the code ones, which is what makes the same
-- thresholds mean something different — and it is the difference the design
-- intends to carry.
