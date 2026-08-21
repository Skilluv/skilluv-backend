-- The ops domain: score, missions, contests, awards, seeded challenges.
-- Migration 0244.
--
-- ## Three tickets that needed no table
--
-- `ops_missions` (M-01) is `missions` with `skill_domain = 'ops'` and mission
-- types of its own. A parallel table would have meant a second application
-- flow, a second invoice path and a second place to hold an escrow — for a
-- mission that differs only in what it asks for.
--
-- Ops contests (C-01) are `tournaments` with two more kinds. Ops awards
-- (C-02) are `award_categories` with a domain. Both mechanisms exist and
-- both were built to take a domain.
--
-- What is genuinely new is the score: ops work is counted differently from
-- code work, and the terms say how.

-- ═══════════════════════════════════════════════════════════════════
-- What an ops score counts
-- ═══════════════════════════════════════════════════════════════════
--
-- The weights say what this domain values, and they are rows so the answer
-- can be argued with rather than found in a function.
--
-- Two choices worth defending. An incident led is worth more than an
-- artefact shipped, because the artefact can be rewritten and the night
-- cannot be replayed. And a cost reduction is scored on the log of what it
-- saved: saving a million a year is worth about twice saving a thousand, not
-- a thousand times, because the second one is often just a bigger bill to
-- start with.

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order)
VALUES
    ('ops', 'attestations_ops', 5, 'count', NULL,
     'Chaque attestation ops délivrée.', 10),
    ('ops', 'infra_artifacts_shipped', 40, 'count', NULL,
     'Chaque module, chart ou pipeline livré et réutilisable par quelqu''un '
     'd''autre.', 20),
    ('ops', 'objectives_met', 60, 'count', NULL,
     'Chaque objectif de service tenu sur sa fenêtre, chiffre à l''appui.', 30),
    ('ops', 'incidents_led', 90, 'count', NULL,
     'Chaque incident conduit avec post-mortem publié. Vaut plus qu''un '
     'artefact : un module se réécrit, une nuit ne se rejoue pas.', 40),
    ('ops', 'migrations_completed', 70, 'count', NULL,
     'Chaque migration majeure menée à son terme — base, cloud, cluster.', 50),
    ('ops', 'observability_stacks_shipped', 50, 'count', NULL,
     'Chaque pile d''observabilité livrée et adoptée.', 60),
    ('ops', 'cost_saved_annual', 40, 'log_scaled', NULL,
     'Les économies annuelles cumulées, sur une échelle logarithmique : un '
     'million vaut environ le double de mille, parce que la deuxième est '
     'souvent une facture plus grosse au départ.', 70),
    ('ops', 'platforms_distinct', 25, 'count', NULL,
     'Chaque plateforme sur laquelle un artefact vérifié tourne.', 80),
    ('ops', 'missions_completed', 100, 'count', NULL,
     'Chaque mission ops payée menée à son terme.', 90),
    ('ops', 'review_grid_average', 200, 'offset_scaled', 3.0,
     'La moyenne des grilles de relecture, comptée à partir de 3 sur 5.', 100),
    ('ops', 'years_active', 25, 'count', NULL,
     'Chaque année depuis le premier artefact vérifié.', 110),
    ('ops', 'featured_times', 200, 'count', NULL,
     'Chaque mise en avant par la communauté.', 120)
ON CONFLICT DO NOTHING;

INSERT INTO craft_score_tiers
    (skill_domain, slug, name, min_score, max_score, description, sort_order)
VALUES
    ('ops', 'apprentice', 'Apprenti', 0, 99,
     'Les premiers artefacts. Rien n''a encore tourné en production.', 1),
    ('ops', 'operator', 'Opérateur', 100, 499,
     'Fait tourner ce que d''autres ont construit, et sait quand appeler.', 2),
    ('ops', 'engineer', 'Ingénieur', 500, 1499,
     'Construit et tient. A conduit des incidents et écrit ce qui en est '
     'sorti.', 3),
    ('ops', 'senior', 'Senior', 1500, 3499,
     'Des systèmes que d''autres exploitent sans le demander, et des '
     'objectifs tenus sur la durée.', 4),
    ('ops', 'staff', 'Staff', 3500, 6999,
     'Une pratique installée : la manière dont une organisation exploite ses '
     'systèmes porte sa marque.', 5),
    ('ops', 'principal', 'Principal', 7000, NULL,
     'Une contribution qui a changé la façon dont d''autres équipes '
     'travaillent.', 6)
ON CONFLICT (skill_domain, slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Ops missions
-- ═══════════════════════════════════════════════════════════════════
--
-- Types of work rather than a second missions table. What differs between an
-- ops mission and a code one is what is asked for and how it is paid — a
-- retainer for on-call is normal here and unusual elsewhere — and both are
-- already columns on `missions`.

INSERT INTO mission_types (slug, skill_domain, name, description, sort_order)
VALUES
    ('ops_infra_build', 'ops', 'Construction d''infrastructure',
     'Monter ou reprendre une infrastructure : modules, cluster, pipeline. '
     'Payée au forfait, sur un livrable qui s''applique deux fois.', 10),
    ('ops_migration', 'ops', 'Migration',
     'Déplacer un système : base, cloud, orchestrateur. Le type de mission '
     'où le retour en arrière fait partie du livrable. Payée au forfait.', 20),
    ('ops_observability', 'ops', 'Mise en observabilité',
     'Instrumenter, alerter, outiller, jusqu''à ce que les questions '
     'd''exploitation aient des réponses. Payée au forfait.', 30),
    ('ops_cost_review', 'ops', 'Revue de coûts',
     'Analyser une facture cloud et réduire ce qui peut l''être sans casser '
     'le service. Payée au forfait, ou sur une part de l''économie tenue.', 40),
    ('ops_oncall_retainer', 'ops', 'Astreinte',
     'Être joignable, avec un délai de réponse convenu. Payée au mois, parce '
     'qu''être disponible est du travail même les nuits où rien ne tombe.', 50),
    ('ops_reliability_review', 'ops', 'Revue de fiabilité',
     'Auditer une architecture sous l''angle de ce qui tombe et de ce qui se '
     'passe ensuite. Payée au forfait.', 60)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Ops contests
-- ═══════════════════════════════════════════════════════════════════
--
-- Two rows on `tournament_kinds`. Migration 0516 made the formats a table
-- because the CHECK had been rewritten three times and one rewrite deleted
-- two of its predecessor's values — so a domain adding a format no longer
-- retypes every other domain's.
--
-- Neither of these is measured by a jury. A chaos weekend is scored on what
-- the exercise revealed, which entrants write up; a cost hunt is scored on
-- money removed with the service still standing, which is a number and a
-- verification. Both hand something in, and both have to say up front what
-- system is in scope — a chaos contest with no named target is an invitation
-- to break something that was not offered.

INSERT INTO tournament_kinds
    (slug, skill_domain, name, description, expects_submission, is_measured,
     lower_is_better, required_rule_keys, sort_order) VALUES
    ('chaos_weekend', 'ops', 'Week-end du chaos',
     'Un système offert, un week-end, et ce que la panne provoquée a révélé. '
     'Ce qui est jugé est le compte rendu, pas la casse.',
     TRUE, FALSE, FALSE, '{target_system}', 110),
    ('cost_hunt', 'ops', 'Chasse aux coûts',
     'Une facture, une semaine, et le montant retiré — avec la preuve que le '
     'service tient toujours. Une économie qui a cassé le service est une '
     'panne avec un tableur.',
     TRUE, TRUE, FALSE, '{target_system,baseline_bill}', 120)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Ops awards
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO award_categories (slug, name, description, subject_type, sort_order)
VALUES
    ('ops_runbook_of_the_year', 'Runbook de l''année',
     'Le document qu''on est content de trouver à trois heures du matin.',
     'deliverable', 210),
    ('ops_postmortem_of_the_year', 'Post-mortem de l''année',
     'Celui qui a appris quelque chose à des gens qui n''étaient pas là.',
     'deliverable', 220),
    ('ops_module_of_the_year', 'Module de l''année',
     'L''artefact d''infrastructure le plus repris par d''autres.',
     'deliverable', 230),
    ('ops_saving_of_the_year', 'Économie de l''année',
     'La réduction de coûts la mieux documentée, service intact.',
     'deliverable', 240),
    ('ops_quiet_year', 'L''année tranquille',
     'Le service dont personne n''a parlé parce qu''il n''est jamais tombé. '
     'La catégorie que le métier mérite et que personne ne décerne. Elle '
     'récompense la personne qui l''a tenu, pas l''artefact.',
     'user', 250)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- What an ops reviewer looks at
-- ═══════════════════════════════════════════════════════════════════
--
-- Five families, five grids, plus the domain default that applies when
-- nothing narrower does. They are the machine-readable half of
-- docs/ops/REVIEW-GRIDS.md, and a seeded challenge copies the matching one
-- as its rubric — a challenge sent to a reviewer without one asks whether the
-- work is good without ever saying what good means.
--
-- The three refusals at the top of the domain grid are not criteria to score.
-- A secret in a repository, an access wider than needed, or nothing to roll
-- back to: none of them is compensated by the quality of the rest.

INSERT INTO review_grids (domain, reviewer_group, display_name, criteria) VALUES

('ops', NULL, 'Ops — critères communs', '[
  {"criterion": "Aucun secret en clair", "looks_like": "Ni dans le dépôt, ni dans un manifeste, ni dans une variable d''environnement commitée, ni dans une capture d''écran. Refus sans discussion."},
  {"criterion": "Le moindre accès nécessaire", "looks_like": "Pas de rôle administrateur « en attendant », pas de groupe de sécurité ouvert à 0.0.0.0/0, pas de compte de service partagé. Refus sans discussion."},
  {"criterion": "Un retour en arrière existe", "looks_like": "Un plan de retour, ou une sauvegarde vérifiée avant une opération destructive. Refus sans discussion."},
  {"criterion": "Utilisable sans son auteur", "looks_like": "Quelqu''un d''autre s''en sert en suivant la documentation, sans poser de question à celui qui l''a écrit."},
  {"criterion": "Ce qui est annoncé est mesuré", "looks_like": "Un chiffre vient avec sa source et sa fenêtre. « Fiable » n''est pas une preuve."},
  {"criterion": "Transparence sur l''IA", "looks_like": "L''usage d''un assistant est déclaré. Il est accepté ; le camoufler ne l''est pas."}
]'),

('ops', 'infra', 'Infra — grille de revue', '[
  {"criterion": "Reproductible", "looks_like": "Deux exécutions donnent le même état. Rien ne dépend de ce qui était là avant. Un apply qui ne peut être joué qu''une fois est refusé."},
  {"criterion": "Paramétré, pas copié", "looks_like": "Les valeurs qui changent sont des variables, avec des valeurs par défaut sûres."},
  {"criterion": "Le plan est lisible", "looks_like": "Le plan montre ce qui va se passer, sans ressource surprise."},
  {"criterion": "Destruction testée", "looks_like": "Ce qui est créé peut être détruit sans laisser d''orphelins."},
  {"criterion": "Documentation d''usage", "looks_like": "Quelqu''un d''autre l''utilise sans lire le code."},
  {"criterion": "Versions épinglées", "looks_like": "Providers, images, charts. Une version flottante est une panne différée."}
]'),

('ops', 'reliability', 'Fiabilité — grille de revue', '[
  {"criterion": "L''objectif est mesurable", "looks_like": "Une cible, une fenêtre, et une source de mesure nommée que le relecteur peut ouvrir."},
  {"criterion": "L''objectif est atteignable", "looks_like": "Une cible que l''architecture rend impossible est un mensonge poli."},
  {"criterion": "Le budget d''erreur sert", "looks_like": "Un budget jamais entamé signale une cible trop basse, et se dit."},
  {"criterion": "Le runbook est jouable", "looks_like": "Écrit pour quelqu''un qui n''a pas construit le système, à trois heures du matin. Un runbook qui commence par « demander à » est refusé."},
  {"criterion": "Le post-mortem porte sur le système", "looks_like": "Ce que le système a permis, pas qui a tapé quoi."},
  {"criterion": "Les actions ont un porteur et une date", "looks_like": "Sinon elles n''existent pas."}
]'),

('ops', 'cloud', 'Cloud — grille de revue', '[
  {"criterion": "Le coût est chiffré", "looks_like": "Une facture estimée poste par poste, avec les hypothèses de charge. Une architecture sans chiffre est une architecture qu''on découvrira."},
  {"criterion": "Les compromis sont écrits", "looks_like": "Ce qui a été choisi, ce qui a été écarté, et la raison."},
  {"criterion": "L''enfermement est nommé", "looks_like": "Ce qu''il faudrait réécrire pour changer de fournisseur, et pourquoi c''est accepté."},
  {"criterion": "La reprise est décrite", "looks_like": "RTO, RPO, et le test qui les a vérifiés — pas seulement visés."},
  {"criterion": "La région est justifiée", "looks_like": "Latence, souveraineté des données, coût. Pas « c''est la région par défaut »."},
  {"criterion": "Le schéma dit la vérité", "looks_like": "Un multi-région dont la base est mono-région le dit."}
]'),

('ops', 'observability', 'Observabilité — grille de revue', '[
  {"criterion": "L''alerte est actionnable", "looks_like": "Elle dit quoi faire, ou pointe vers le runbook qui le dit."},
  {"criterion": "L''alerte réveille pour une raison", "looks_like": "Elle part d''un symptôme utilisateur. Un seuil de ressource sans lien avec un symptôme est refusé."},
  {"criterion": "La cardinalité est maîtrisée", "looks_like": "Une étiquette par identifiant utilisateur est une facture, pas une métrique."},
  {"criterion": "Les traces relient", "looks_like": "Une requête se suit d''un bout à l''autre, y compris à travers les files."},
  {"criterion": "Le tableau de bord répond à une question", "looks_like": "Un mur de graphiques n''est pas de l''observabilité."},
  {"criterion": "La rétention est décidée", "looks_like": "Combien de temps, pourquoi, et ce que ça coûte."}
]'),

('ops', 'data', 'Données — grille de revue', '[
  {"criterion": "La migration est réversible", "looks_like": "Ou explicitement irréversible, avec la sauvegarde vérifiée avant et par qui."},
  {"criterion": "Le verrou est borné", "looks_like": "La durée du verrou est mesurée, et le volume de la table est écrit. Un ALTER TABLE sans volume est refusé."},
  {"criterion": "Le plan de requête est joint", "looks_like": "Avant et après, sur des volumes réalistes."},
  {"criterion": "L''index sert", "looks_like": "Une requête l''utilise. Un index sans requête est un coût d''écriture permanent."},
  {"criterion": "La restauration a été testée", "looks_like": "Une sauvegarde jamais restaurée n''est pas une sauvegarde."},
  {"criterion": "Le décalage de réplication est surveillé", "looks_like": "Avec le seuil au-delà duquel une bascule est refusée."}
]')

ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Seeded challenges, one set per trade
-- ═══════════════════════════════════════════════════════════════════
--
-- Drafts, as every seeded challenge is: a human publishes them after reading
-- them. Each one asks for an artefact somebody else could use, because that
-- is what this domain's proof looks like — and each one carries the grid of
-- its family, so a submission is read against criteria its author could read
-- first.

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
    'Dans tous les cas : le livrable est utilisable par quelqu''un d''autre ' ||
    'sans toi dans la pièce, et ce qui ne marche pas encore est écrit par son ' ||
    'auteur. Un artefact sans mode d''emploi est refusé.' || E'\n\n' ||
    '## Ce qui sera regardé' || E'\n\n' ||
    'La grille de relecture de la famille s''applique, et elle est publique : ' ||
    'tu peux la lire avant de soumettre.',
    'ops', c.difficulty,
    'draft', TRUE,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'ops' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'ops' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

-- ── devops-engineer (4) ────────────────────────────────────────────
('devops-engineer', 'Un pipeline depuis rien',
 'Prendre un dépôt sans intégration continue et lui donner un pipeline qui construit, teste et publie une image',
 'Le fichier de pipeline, et un README que quelqu''un d''autre suit sans toi.', 2),

('devops-engineer', 'Sortir les secrets du dépôt',
 'Un dépôt contient des secrets en clair : les sortir, faire tourner ce qui doit l''être, documenter la rotation suivante',
 'Le dépôt nettoyé, le magasin de secrets en place, et la procédure de rotation écrite.', 3),

('devops-engineer', 'Un module qui s''applique deux fois',
 'Écrire un module d''infrastructure qui, appliqué deux fois de suite, ne montre aucune différence',
 'Le module, et la trace des deux exécutions dans le README.', 3),

('devops-engineer', 'Un retour en arrière qui marche',
 'Ajouter à un déploiement existant un chemin de retour testé',
 'Le mécanisme, la trace du test, et la durée mesurée.', 4),

-- ── sre (4) ────────────────────────────────────────────────────────
('sre', 'Un premier objectif de service',
 'Choisir un service, écrire un objectif mesurable, l''instrumenter et le tenir trente jours',
 'La cible, la source de mesure accessible au relecteur, et le résultat de la fenêtre.', 2),

('sre', 'Une politique de budget d''erreur',
 'Écrire ce qui se passe quand le budget est épuisé : ce qu''on arrête, ce qu''on priorise',
 'Le document, et la trace de son acceptation par une équipe.', 3),

('sre', 'Le runbook du pire cas',
 'Documenter le mode de panne le plus grave d''un système',
 'Un runbook jouable par quelqu''un qui n''a pas construit le système.', 3),

('sre', 'Une journée de panne organisée',
 'Provoquer une panne en environnement contrôlé et mesurer la détection puis la reprise',
 'Le scénario, les durées mesurées, et ce qui a manqué.', 4),

-- ── cloud-architect (4) ────────────────────────────────────────────
('cloud-architect', 'Une architecture chiffrée',
 'Concevoir une architecture et joindre la facture estimée poste par poste',
 'Le schéma, les hypothèses de charge, et le coût mensuel estimé.', 3),

('cloud-architect', 'Nommer l''enfermement',
 'Prendre une architecture existante et écrire ce qu''il faudrait réécrire pour changer de fournisseur',
 'La liste, le coût estimé de la sortie, et pourquoi c''est accepté.', 3),

('cloud-architect', 'Une reprise testée',
 'Définir RTO et RPO pour un système, puis jouer la reprise',
 'Les cibles, la procédure, et les durées réellement obtenues.', 4),

('cloud-architect', 'Le choix de la région',
 'Justifier une région par la latence mesurée, la souveraineté des données et le coût',
 'Les mesures, la contrainte réglementaire s''il y en a une, et la décision.', 2),

-- ── platform-engineer (4) ──────────────────────────────────────────
('platform-engineer', 'Un chemin par défaut',
 'Créer le chemin le plus court pour qu''une équipe mette un service en production',
 'Le chemin, et la trace de quelqu''un qui n''est pas toi l''ayant suivi.', 3),

('platform-engineer', 'Un environnement en libre-service',
 'Permettre à une équipe de créer son environnement sans ticket, avec des garde-fous',
 'Le mécanisme, le plafond de coût, et la durée de vie automatique.', 4),

('platform-engineer', 'La documentation du chemin',
 'Écrire la documentation qui répond aux cinq questions les plus posées, mesurées et pas devinées',
 'La mesure des questions, et la documentation qui y répond.', 2),

('platform-engineer', 'Mesurer la friction',
 'Mesurer le temps entre un commit et sa mise en production, puis en retirer un tiers',
 'La mesure avant, ce qui a été changé, et la mesure après.', 4),

-- ── kubernetes-specialist (4) ──────────────────────────────────────
('kubernetes-specialist', 'Un premier opérateur',
 'Écrire un opérateur qui gère un objet simple de bout en bout',
 'Le code, les tests, et le README d''installation.', 4),

('kubernetes-specialist', 'Un chart qui se met à jour',
 'Publier un chart dont la montée de version conserve les données',
 'Le chart, et la trace d''une montée de version avec données préservées.', 3),

('kubernetes-specialist', 'GitOps depuis zéro',
 'Passer un cluster de déploiements manuels à une réconciliation par dépôt, sans coupure',
 'La configuration, et la trace de la bascule sans interruption.', 4),

('kubernetes-specialist', 'Des limites qui tiennent',
 'Donner des requêtes et des limites justifiées par une mesure à des charges existantes',
 'Les mesures, les valeurs retenues, et la méthode pour les revoir.', 3),

-- ── observability-engineer (4) ─────────────────────────────────────
('observability-engineer', 'Trois questions, deux minutes',
 'Choisir trois questions d''exploitation et construire ce qu''il faut pour y répondre en moins de deux minutes chacune',
 'Les trois questions, les tableaux de bord, et le chronométrage.', 3),

('observability-engineer', 'Une alerte qui mérite de réveiller',
 'Remplacer une alerte de seuil de ressource par une alerte de symptôme utilisateur',
 'L''ancienne alerte, la nouvelle, et la baisse de bruit mesurée.', 3),

('observability-engineer', 'Une trace de bout en bout',
 'Instrumenter une requête à travers au moins trois services, files comprises',
 'L''instrumentation, et une trace complète d''une requête réelle.', 4),

('observability-engineer', 'La facture de la cardinalité',
 'Trouver l''étiquette qui coûte le plus cher dans une pile de métriques et la retirer sans perdre la réponse qu''elle servait',
 'L''analyse, le changement, et la facture avant et après.', 4),

-- ── incident-commander (3) ─────────────────────────────────────────
('incident-commander', 'Conduire un exercice',
 'Conduire un exercice d''incident : rôles, communication, chronologie',
 'Le scénario, la chronologie réelle, et le compte rendu.', 3),

('incident-commander', 'Un post-mortem sans blâme',
 'Écrire le post-mortem d''un incident réel en expliquant ce que le système a permis',
 'Le document, sans nom de personne, avec des actions portées et datées.', 3),

('incident-commander', 'Communiquer pendant',
 'Écrire les modèles de communication d''incident : interne, client, public',
 'Les trois modèles, relus par quelqu''un qui n''est pas technique.', 2),

-- ── database-administrator (4) ─────────────────────────────────────
('database-administrator', 'Une migration sans verrou',
 'Modifier une table volumineuse en production sans verrou long',
 'La procédure, le volume de la table, et la durée du verrou mesurée.', 4),

('database-administrator', 'Un index qui se justifie',
 'Ajouter un index à partir d''un plan de requête et mesurer les deux côtés',
 'Le plan avant et après, le gain en lecture et le coût en écriture.', 3),

('database-administrator', 'Restaurer pour de vrai',
 'Restaurer une base depuis une sauvegarde et chronométrer',
 'La procédure, la durée obtenue, et ce qui a manqué.', 3),

('database-administrator', 'Le décalage de réplication',
 'Instrumenter le décalage d''une réplication et définir le seuil de refus de bascule',
 'L''instrumentation, le seuil justifié, et l''alerte.', 4)

) AS c(orientation_slug, title, description, expected, difficulty)
JOIN orientations o ON o.slug = c.orientation_slug
WHERE NOT EXISTS (
    SELECT 1 FROM challenge_templates t
     WHERE t.title = c.title AND t.skill_domain = 'ops'
);
