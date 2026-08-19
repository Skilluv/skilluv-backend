-- The second tier of ops challenges.
--
-- Migration 0244 seeded the first artefact of each trade: the pipeline, the
-- first objective, the first operator. These are the ones somebody takes
-- afterwards, and they are seeded separately because the two sets answer
-- different questions. The first asks "can you produce something another
-- person can run"; this one asks "can you do the thing the job actually
-- consists of".
--
-- ## Why some obvious ones are absent
--
-- No "set up a service mesh on production traffic", no "upgrade a live
-- cluster". Both appear below in a form somebody can actually attempt: on a
-- cluster of their own, with the failure modes demonstrated rather than
-- described. A challenge nobody without an employer can attempt is a
-- challenge that filters on employment, and this platform exists for the
-- people that filter excludes.

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty,
     status, is_training, evaluation_rubric)
SELECT
    c.title,
    c.description,
    '## Ce qu''il y a à faire' || E'\n\n' ||
    c.description || E'.\n\n' ||
    '## Ce qui est attendu' || E'\n\n' ||
    c.expected || E'\n\n' ||
    'Ce défi suppose que tu as déjà livré un artefact dans ce métier. Tu ' ||
    'peux tout faire sur ta propre machine ou sur un palier gratuit : si tu ' ||
    'te retrouves à avoir besoin de la production de quelqu''un, c''est que ' ||
    'le périmètre a glissé.' || E'\n\n' ||
    '## Ce qui sera regardé' || E'\n\n' ||
    'La grille de relecture de la famille s''applique, et elle est publique.',
    'ops', c.difficulty,
    'draft', FALSE,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'ops' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'ops' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

-- ── devops-engineer ────────────────────────────────────────────────
('devops-engineer', 'Une image dix fois plus petite',
 'Réduire une image de conteneur d''un ordre de grandeur par une construction en plusieurs étapes, sans perdre ce dont l''exploitation a besoin pour diagnostiquer',
 'Le Dockerfile, les deux tailles, et la liste de ce qui reste installé avec la raison de chaque entrée.', 3),

('devops-engineer', 'Une chaîne de livraison qui refuse de livrer',
 'Ajouter à un pipeline les contrôles qui bloquent : analyse de dépendances, recherche de secrets, échec sur régression de couverture',
 'Le pipeline, et trois exécutions qui échouent volontairement, une par contrôle.', 4),

-- ── sre ────────────────────────────────────────────────────────────
('sre', 'Une astreinte tenable',
 'Concevoir une rotation d''astreinte pour une équipe donnée : plages, remplaçant, escalade, et ce qui est payé',
 'Le document, relu par quelqu''un qui a déjà été d''astreinte, et la charge estimée par personne et par mois.', 4),

('sre', 'Un test de charge et ce qu''il révèle',
 'Charger un service avec k6 ou Gatling jusqu''à trouver ce qui casse en premier, puis planifier la capacité à partir de ce chiffre',
 'Le scénario, la ressource qui sature en premier, le seuil, et la projection à douze mois.', 4),

-- ── cloud-architect ────────────────────────────────────────────────
('cloud-architect', 'Un tiers de facture en moins',
 'Auditer une facture cloud réelle et retirer au moins trente pour cent, en démontrant que le service tient toujours',
 'Les deux factures, le détail des changements, et la mesure de service avant et après.', 4),

('cloud-architect', 'Une architecture sans serveur, chiffrée honnêtement',
 'Concevoir et livrer une architecture sans serveur, avec le coût au million de requêtes et le démarrage à froid mesuré',
 'Le code déployable, la facture au volume, et les latences de démarrage à froid réellement observées.', 4),

-- ── platform-engineer ──────────────────────────────────────────────
('platform-engineer', 'Les quatre chiffres de DORA',
 'Instrumenter les quatre métriques DORA sur un dépôt réel, à partir de ses données de livraison, et publier le tableau',
 'La méthode de calcul de chacune, la source des données, et un mois de mesure.', 4),

('platform-engineer', 'Un environnement de développement reproductible',
 'Rendre l''installation d''un projet reproductible avec Nix, un conteneur de développement ou équivalent, sur trois systèmes',
 'La configuration, et la trace de trois personnes sur trois systèmes différents ayant obtenu le même état.', 3),

-- ── kubernetes-specialist ──────────────────────────────────────────
('kubernetes-specialist', 'Un maillage de services et ce qu''il coûte',
 'Installer un maillage sur un cluster à toi, poser une politique de trafic, et mesurer ce que la couche ajoute en latence et en mémoire',
 'La configuration, la politique, et les mesures avant et après installation.', 5),

('kubernetes-specialist', 'Une montée de version sans coupure',
 'Faire monter un cluster de deux versions mineures avec une charge en cours, sans interruption observée par le client',
 'La procédure, la sonde qui mesure l''interruption pendant l''opération, et son résultat.', 5),

-- ── observability-engineer ─────────────────────────────────────────
('observability-engineer', 'Des journaux qui ne coûtent pas une fortune',
 'Mettre en place une agrégation de journaux et diviser son coût d''ingestion, par échantillonnage, rétention par niveau ou étiquetage',
 'La configuration, le volume avant et après, et ce qu''on a perdu comme capacité de réponse.', 4),

('observability-engineer', 'Une lenteur trouvée par la trace',
 'Prendre une lenteur réelle et la remonter jusqu''à sa cause par le traçage distribué, en publiant le raisonnement',
 'La trace, le raisonnement étape par étape, la correction, et la mesure après.', 4),

-- ── incident-commander ─────────────────────────────────────────────
('incident-commander', 'Un manuel de réponse complet',
 'Écrire le manuel de réponse d''une organisation : échelle de gravité, rôles, communication, escalade, et qui décide quoi',
 'Le document, et la trace d''un exercice joué contre lui par des gens qui ne l''ont pas écrit.', 4),

('incident-commander', 'Animer un post-mortem',
 'Écrire le guide d''animation d''un post-mortem sans blâme, et l''utiliser pour en animer un réel',
 'Le guide, le post-mortem qui en est sorti, et ce que les participants en ont dit.', 4),

-- ── database-administrator ─────────────────────────────────────────
('database-administrator', 'Une bascule jouée pour de vrai',
 'Monter une réplication, provoquer la panne du primaire, et mesurer ce que la bascule coûte en temps et en écritures perdues',
 'La configuration, la durée de bascule mesurée, et le nombre d''écritures perdues — zéro est une réponse à prouver.', 5),

('database-administrator', 'Un entrepôt analytique qui répond en une seconde',
 'Ingérer un jeu de données conséquent dans ClickHouse et obtenir des agrégations sous la seconde, en expliquant le schéma qui le permet',
 'Le schéma, la méthode d''ingestion, et les temps de réponse sur un volume annoncé.', 4)

) AS c(orientation_slug, title, description, expected, difficulty)
JOIN orientations o ON o.slug = c.orientation_slug
WHERE NOT EXISTS (
    SELECT 1 FROM challenge_templates t
     WHERE t.title = c.title AND t.skill_domain = 'ops'
);
