-- The ops skill tree, and which trade needs which part of it.
--
-- Migration 0057 seeded thirty-nine ops skills, written when the domain was
-- "the things a developer has to know to ship": git, a pipeline, a
-- Dockerfile, a dashboard. Five of the eight trades did not exist then, and
-- the skills that define them are absent — Terraform, operators, error
-- budgets, replication.
--
-- ## Why the map matters more than the list
--
-- A skill node with no orientation pointing at it is a word in a glossary.
-- What makes the tree useful is `orientation_skill_map`: somebody who picks
-- `database-administrator` should be shown replication and query plans, not
-- Helm. Without the map the recommendation engine has to guess from the
-- domain, and the domain is eight trades wide.
--
-- Core against recommended is a real distinction rather than a ranking: core
-- means somebody without it cannot do the job at all.

-- ═══════════════════════════════════════════════════════════════════
-- The families the new skills hang from
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain) VALUES
('infrastructure-as-code', 'Infrastructure comme code',
 'Décrire une infrastructure dans un dépôt plutôt que dans une console. La propriété qui compte est qu''appliquer deux fois ne change rien.', 'ops'),
('kubernetes', 'Kubernetes',
 'Orchestration de conteneurs. Rentable au-delà d''une certaine taille, et le dire fait partie du métier.', 'ops'),
('site-reliability', 'Fiabilité de service',
 'Objectifs, budgets d''erreur, résilience. Décider combien d''indisponibilité est acceptable et organiser le travail autour.', 'ops'),
('cloud-architecture', 'Architecture cloud',
 'Concevoir, chiffrer, et écrire les compromis. Un schéma sans facture estimée est une architecture qu''on découvrira.', 'ops'),
('database-operations', 'Exploitation de bases de données',
 'Réplication, réglage, migrations, reprise. Le seul endroit en ops où une erreur ne se rattrape pas par un redéploiement.', 'ops'),
('platform-engineering', 'Ingénierie de plateforme',
 'La plateforme interne sur laquelle les autres équipes livrent : chemins par défaut, libre-service, friction mesurée.', 'ops'),
('secrets-management', 'Gestion des secrets',
 'Magasins, rotation, identifiants à durée de vie courte. Le premier motif de refus en relecture ops.', 'ops')
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- What each family contains
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT c.slug, c.display_name, c.description, 'ops', p.id
FROM (VALUES

-- Infrastructure as code
('terraform',              'Terraform / OpenTofu',        'Le langage de description le plus répandu, et ses états.', 'infrastructure-as-code'),
('terraform-module-design','Conception de modules',       'Variables plutôt que copies, défauts sûrs, versions épinglées.', 'infrastructure-as-code'),
('terraform-state',        'Gestion de l''état',           'Où vit l''état, qui le verrouille, et ce qui casse quand deux personnes appliquent.', 'infrastructure-as-code'),
('ansible',                'Ansible',                     'Configuration sans agent, par SSH. Ce qu''on utilise quand il y a des machines.', 'infrastructure-as-code'),
('idempotent-provisioning','Provisionnement idempotent',  'Appliquer deux fois ne change rien. La propriété qui sépare l''IaC du script.', 'infrastructure-as-code'),

-- Kubernetes
('k8s-manifests',          'Manifestes Kubernetes',       'Deployment, Service, Ingress. À écrire à la main avant d''utiliser un chart.', 'kubernetes'),
('helm-charts',            'Charts Helm',                 'Empaqueter, et faire en sorte qu''une montée de version conserve les données.', 'kubernetes'),
('k8s-operators',          'Opérateurs',                  'Kubebuilder, operator-sdk. Automatiser ce qu''un humain ferait à répétition.', 'kubernetes'),
('gitops',                 'GitOps',                      'Argo CD, Flux. Le déploiement cesse d''être un geste et devient un état écrit.', 'kubernetes'),
('service-mesh',           'Maillage de services',        'Istio, Linkerd. Une couche qui coûte en latence et en mémoire : la mesurer.', 'kubernetes'),
('k8s-resource-limits',    'Requêtes et limites',         'Justifiées par une mesure, pas par une estimation.', 'kubernetes'),
('k8s-cluster-upgrades',   'Montées de version de cluster','Sans coupure, avec une sonde qui mesure l''interruption pendant.', 'kubernetes'),

-- Reliability
('slo-definition',         'Définition d''objectifs',      'Une cible, une fenêtre, une source de mesure nommée.', 'site-reliability'),
('error-budget-policy',    'Politique de budget d''erreur','Ce qu''on arrête quand le budget est épuisé, décidé avant.', 'site-reliability'),
('chaos-engineering',      'Ingénierie du chaos',         'Provoquer une panne avec une hypothèse écrite avant. Sans hypothèse c''est une panne.', 'site-reliability'),
('load-testing',           'Test de charge',              'k6, Gatling. Trouver ce qui sature en premier, et à quel niveau.', 'site-reliability'),
('capacity-planning',      'Planification de capacité',   'Projeter la croissance sur la ressource qui casse en premier.', 'site-reliability'),
('postmortem-facilitation','Animation de post-mortem',    'Sans blâme, avec des actions portées et datées.', 'site-reliability'),
('oncall-rotation-design', 'Conception d''astreinte',      'Plages, remplaçant, escalade, et ce qui est payé. Une rotation de un n''en est pas une.', 'site-reliability'),

-- Cloud
('cloud-cost-analysis',    'Analyse de coûts',            'Lire une facture poste par poste, et savoir ce qu''on paie.', 'cloud-architecture'),
('multi-region-design',    'Conception multi-région',     'RTO, RPO, et le test qui les a vérifiés plutôt que visés.', 'cloud-architecture'),
('vendor-lock-in-analysis','Analyse de l''enfermement',    'Ce qu''il faudrait réécrire pour partir, et pourquoi c''est accepté.', 'cloud-architecture'),
('serverless-architecture','Architecture sans serveur',   'Coût au million de requêtes, et démarrage à froid mesuré.', 'cloud-architecture'),
('disaster-recovery',      'Reprise après sinistre',      'Une procédure jouée, avec les durées réellement obtenues.', 'cloud-architecture'),

-- Databases
('query-plan-reading',     'Lecture de plans',            'EXPLAIN ANALYZE sur des volumes réalistes. Sur mille lignes rien ne s''apprend.', 'database-operations'),
('index-design',           'Conception d''index',          'Le gain en lecture et le coût en écriture, mesurés tous les deux.', 'database-operations'),
('zero-downtime-migration','Migration sans verrou',       'Avec le volume de la table et la durée du verrou mesurée.', 'database-operations'),
('db-replication',         'Réplication',                 'Et le décalage surveillé avant d''en avoir besoin.', 'database-operations'),
('backup-restore-drills',  'Exercices de restauration',   'Une sauvegarde jamais restaurée n''est pas une sauvegarde.', 'database-operations'),
('connection-pooling',     'Regroupement de connexions',  'Ce qui manque à presque toute application qui sature sa base.', 'database-operations'),
('columnar-analytics',     'Analytique en colonnes',      'ClickHouse, DuckDB. Le schéma décide de la seconde ou de la minute.', 'database-operations'),

-- Platform
('golden-paths',           'Chemins par défaut',          'Le trajet le plus court vers la production, suivi par quelqu''un d''autre.', 'platform-engineering'),
('self-service-infra',     'Infrastructure en libre-service','Sans ticket, avec un plafond de coût et une durée de vie.', 'platform-engineering'),
('dora-metrics',           'Métriques DORA',              'Quatre chiffres, calculés depuis des données de livraison réelles.', 'platform-engineering'),
('developer-experience',   'Expérience développeur',      'Mesurer la friction avant de la réduire. Le chiffre est toujours pire que ce qu''on croit.', 'platform-engineering'),
('reproducible-dev-env',   'Environnement reproductible', 'Nix, conteneurs de développement. Le même état sur trois machines.', 'platform-engineering'),

-- Secrets
('secret-rotation',        'Rotation des secrets',        'Et la procédure de la fois suivante, écrite.', 'secrets-management'),
('vault-operations',       'Exploitation d''un magasin',    'Vault, SOPS. Des identifiants à durée de vie courte plutôt que permanents.', 'secrets-management'),
('secret-scanning',        'Détection de secrets',        'Dans l''historique aussi, pas seulement dans le dernier commit.', 'secrets-management')

) AS c(slug, display_name, description, parent_slug)
JOIN skill_nodes p ON p.slug = c.parent_slug
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Which trade needs which
-- ═══════════════════════════════════════════════════════════════════
--
-- Core means somebody without it cannot do the job. Everything else is
-- recommended, which is a different statement from "less important": a DBA
-- who has never touched a pipeline is still a DBA.

INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended)
SELECT o.id, s.id, m.is_core, NOT m.is_core
FROM (VALUES

-- ── devops-engineer ────────────────────────────────────────────────
('devops-engineer', 'ci-cd', TRUE),
('devops-engineer', 'containers', TRUE),
('devops-engineer', 'terraform', TRUE),
('devops-engineer', 'idempotent-provisioning', TRUE),
('devops-engineer', 'secret-rotation', TRUE),
('devops-engineer', 'dockerfile-multi-stage', FALSE),
('devops-engineer', 'rollback-strategies', FALSE),
('devops-engineer', 'terraform-module-design', FALSE),
('devops-engineer', 'ansible', FALSE),
('devops-engineer', 'secret-scanning', FALSE),
('devops-engineer', 'k8s-manifests', FALSE),

-- ── platform-engineer ──────────────────────────────────────────────
('platform-engineer', 'golden-paths', TRUE),
('platform-engineer', 'self-service-infra', TRUE),
('platform-engineer', 'developer-experience', TRUE),
('platform-engineer', 'ci-cd', TRUE),
('platform-engineer', 'dora-metrics', FALSE),
('platform-engineer', 'reproducible-dev-env', FALSE),
('platform-engineer', 'terraform-module-design', FALSE),
('platform-engineer', 'k8s-manifests', FALSE),
('platform-engineer', 'cloud-cost-analysis', FALSE),

-- ── kubernetes-specialist ──────────────────────────────────────────
('kubernetes-specialist', 'k8s-manifests', TRUE),
('kubernetes-specialist', 'helm-charts', TRUE),
('kubernetes-specialist', 'k8s-operators', TRUE),
('kubernetes-specialist', 'gitops', TRUE),
('kubernetes-specialist', 'k8s-resource-limits', TRUE),
('kubernetes-specialist', 'service-mesh', FALSE),
('kubernetes-specialist', 'k8s-cluster-upgrades', FALSE),
('kubernetes-specialist', 'containers', FALSE),
('kubernetes-specialist', 'vault-operations', FALSE),

-- ── sre ────────────────────────────────────────────────────────────
('sre', 'slo-definition', TRUE),
('sre', 'error-budget-policy', TRUE),
('sre', 'runbook-writing', TRUE),
('sre', 'observability', TRUE),
('sre', 'incident-response', TRUE),
('sre', 'chaos-engineering', FALSE),
('sre', 'load-testing', FALSE),
('sre', 'capacity-planning', FALSE),
('sre', 'oncall-rotation-design', FALSE),
('sre', 'alerting-design', FALSE),

-- ── incident-commander ─────────────────────────────────────────────
('incident-commander', 'incident-triage', TRUE),
('incident-commander', 'incident-communication', TRUE),
('incident-commander', 'postmortem-facilitation', TRUE),
('incident-commander', 'runbook-writing', TRUE),
('incident-commander', 'oncall-rotation-design', FALSE),
('incident-commander', 'on-call-hygiene', FALSE),
('incident-commander', 'observability', FALSE),
('incident-commander', 'chaos-engineering', FALSE),

-- ── cloud-architect ────────────────────────────────────────────────
('cloud-architect', 'cloud-cost-analysis', TRUE),
('cloud-architect', 'multi-region-design', TRUE),
('cloud-architect', 'vendor-lock-in-analysis', TRUE),
('cloud-architect', 'disaster-recovery', TRUE),
('cloud-architect', 'terraform', FALSE),
('cloud-architect', 'serverless-architecture', FALSE),
('cloud-architect', 'capacity-planning', FALSE),
('cloud-architect', 'infrastructure-provisioning', FALSE),

-- ── observability-engineer ─────────────────────────────────────────
('observability-engineer', 'prometheus-metrics', TRUE),
('observability-engineer', 'distributed-tracing', TRUE),
('observability-engineer', 'alerting-design', TRUE),
('observability-engineer', 'structured-logging', TRUE),
('observability-engineer', 'grafana-dashboards', FALSE),
('observability-engineer', 'log-aggregation-loki', FALSE),
('observability-engineer', 'slo-definition', FALSE),
('observability-engineer', 'k8s-manifests', FALSE),

-- ── database-administrator ─────────────────────────────────────────
('database-administrator', 'query-plan-reading', TRUE),
('database-administrator', 'index-design', TRUE),
('database-administrator', 'zero-downtime-migration', TRUE),
('database-administrator', 'backup-restore-drills', TRUE),
('database-administrator', 'db-replication', TRUE),
('database-administrator', 'connection-pooling', FALSE),
('database-administrator', 'columnar-analytics', FALSE),
('database-administrator', 'backup-restore-postgres', FALSE)

) AS m(orientation_slug, skill_slug, is_core)
JOIN orientations  o ON o.slug = m.orientation_slug
JOIN skill_nodes   s ON s.slug = m.skill_slug
ON CONFLICT DO NOTHING;
