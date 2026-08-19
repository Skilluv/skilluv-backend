-- Skilluv's own infrastructure as an ops terrain.
--
-- The dogfooding argument, applied to a domain where it is unusually strong:
-- a contributor who improves our pipeline can point at a running system and
-- say "that is mine", and the reviewer is the person who operates it. No
-- other terrain gives both.
--
-- ## What this is honest about
--
-- These challenges touch our infrastructure, not our production credentials.
-- Everything below is done in the contributor's own environment against our
-- public repositories, and the result is reviewed and then applied by
-- somebody who already has the access. Migration 0243's charter says no ops
-- mission with production access opens before the reinforced NDA exists;
-- these are how the domain stays alive in the meantime, and they are not a
-- way around it.
--
-- ## Attribution
--
-- Anything merged here produces an attestation like any other artefact, and
-- the fact that Skilluv is the beneficiary changes nothing about how it is
-- reviewed. A platform that graded contributions to itself more generously
-- than contributions elsewhere would be worth exactly nothing to a recruiter.

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
    'Ce défi porte sur l''infrastructure de Skilluv elle-même. Tu travailles ' ||
    'dans ton propre environnement, contre nos dépôts publics ; personne ne ' ||
    'te demandera d''accès à notre production, et si on te le demandait, ce ' ||
    'serait une erreur à signaler.' || E'\n\n' ||
    '## Ce qui sera regardé' || E'\n\n' ||
    'La grille de relecture de la famille s''applique, la même que partout ' ||
    'ailleurs. Une contribution à Skilluv n''est pas relue plus gentiment : ' ||
    'une plateforme qui se noterait mieux elle-même ne vaudrait rien pour ' ||
    'celui qui lit l''attestation.',
    'ops', c.difficulty,
    'draft', FALSE,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'ops' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'ops' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

('devops-engineer', 'Le pipeline de Skilluv, plus court d''un tiers',
 'Mesurer la durée actuelle du pipeline du backend, trouver ce qui la domine, et la réduire d''au moins un tiers sans retirer de test',
 'La mesure avant, ce qui a été changé, la mesure après, et la trace des tests toujours verts.', 3),

('devops-engineer', 'Une image de conteneur plus petite',
 'Réduire la taille de l''image du backend par une construction en plusieurs étapes, sans perdre les outils de diagnostic dont l''exploitation a besoin',
 'Le Dockerfile, les deux tailles, et la liste de ce qui reste installé et pourquoi.', 2),

('kubernetes-specialist', 'Les manifestes de Skilluv',
 'Décrire le déploiement du backend en manifestes ou en chart, avec les sondes de vivacité et de disponibilité qui correspondent vraiment à ce que fait le service',
 'Le chart, une montée de version jouée, et la justification des sondes.', 4),

('kubernetes-specialist', 'Des limites justifiées par une mesure',
 'Proposer des requêtes et limites de ressources pour nos services, à partir d''une mesure sous charge réaliste plutôt que d''une estimation',
 'La méthode de mesure, les chiffres obtenus, et les valeurs retenues.', 3),

('observability-engineer', 'Le tableau de bord de Skilluv',
 'Construire les tableaux de bord Grafana qui répondent aux trois questions qu''on se pose quand la plateforme va mal, à partir des métriques que le backend émet déjà',
 'Les trois questions, les tableaux, et le chronométrage de chaque réponse.', 3),

('observability-engineer', 'Les alertes qui méritent de réveiller',
 'Écrire le jeu d''alertes de Skilluv à partir de symptômes utilisateurs — une inscription qui échoue, un paiement qui ne part pas — et pas de seuils de ressources',
 'Les règles, ce que chacune dit de faire, et le runbook vers lequel elle pointe.', 4),

('observability-engineer', 'Suivre une requête de bout en bout',
 'Instrumenter en OpenTelemetry le trajet complet d''une soumission de livrable, depuis l''API jusqu''aux tâches de fond',
 'L''instrumentation, et une trace complète d''une soumission réelle en environnement de test.', 4),

('database-administrator', 'Les requêtes les plus coûteuses de Skilluv',
 'Prendre les requêtes les plus lentes du backend sur un jeu de données réaliste, et en réparer trois avec les plans à l''appui',
 'Les plans avant et après, et la mesure du coût en écriture de tout index ajouté.', 4),

('database-administrator', 'Une sauvegarde restaurée pour de vrai',
 'Écrire et jouer la procédure de restauration de la base de Skilluv depuis une sauvegarde, sur une base neuve, et chronométrer',
 'La procédure, la durée obtenue, et ce qui a manqué la première fois.', 3),

('sre', 'Le premier objectif de service de Skilluv',
 'Proposer l''objectif de disponibilité de la plateforme : ce qui compte comme requête réussie, sur quelle fenêtre, mesuré où',
 'Le document, la source de mesure branchée, et la politique de budget d''erreur qui va avec.', 3),

('devops-engineer', 'Sortir un secret de notre historique',
 'Auditer nos dépôts publics à la recherche de secrets commités, présents ou passés, et écrire la procédure de rotation',
 'Le rapport, remis en privé et pas en issue publique, et la procédure de rotation.', 3),

('platform-engineer', 'Le chemin le plus court pour contribuer',
 'Mesurer le temps réel entre « je clone le dépôt » et « mes tests passent » pour un nouveau contributeur, puis le réduire',
 'La mesure initiale avec quelqu''un qui découvre vraiment le dépôt, ce qui a été changé, et la mesure après.', 2)

) AS c(orientation_slug, title, description, expected, difficulty)
JOIN orientations o ON o.slug = c.orientation_slug
WHERE NOT EXISTS (
    SELECT 1 FROM challenge_templates t
     WHERE t.title = c.title AND t.skill_domain = 'ops'
);
