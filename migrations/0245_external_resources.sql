-- `external_ai_resources` becomes `external_resources`.
--
-- ## Why the same table
--
-- Migration 0220 built a curated list of tooling, communities and courses for
-- the AI trades, with one column no upstream list has: what it actually takes
-- to reach the thing — free tier, GPU needed, course auditable without
-- paying. That column is the reason the table exists, and it is exactly as
-- necessary for the ops domain, where the answer to "can I practise this"
-- is a cloud bill.
--
-- Two of the ops backlog's tickets were separate tables: a toolkit page
-- (G-02) and `external_cloud_programs` (T-03). Both are this row with a
-- different category. A second table would have meant a second curation
-- workflow, a second admin screen, and a second place for a dead link to
-- survive.
--
-- ## What is new
--
-- `domain`, so the AI toolkit and the ops toolkit are two queries against one
-- table rather than two tables. Existing rows are AI, which is what they were
-- when they were written.
--
-- The category list grows by the ops half. `cloud_free_tier` is its own
-- category rather than an `access_note` on a compute row: what a free tier
-- gives you, and what happens when you cross it, is the whole content of the
-- entry.

ALTER TABLE external_ai_resources RENAME TO external_resources;

ALTER INDEX idx_external_ai_resources_category
    RENAME TO idx_external_resources_category;
ALTER INDEX idx_external_ai_resources_orientations
    RENAME TO idx_external_resources_orientations;

ALTER TABLE external_resources
    ADD COLUMN domain VARCHAR(30) NOT NULL DEFAULT 'ai';

-- The default was for the backfill. Leaving it in place would make the next
-- domain's first forgotten insert land silently in the AI toolkit.
ALTER TABLE external_resources ALTER COLUMN domain DROP DEFAULT;

ALTER TABLE external_resources
    DROP CONSTRAINT IF EXISTS external_ai_resources_category_check;

ALTER TABLE external_resources
    ADD CONSTRAINT external_resources_category_check
    CHECK (category IN (
        -- AI (migration 0220).
        'framework', 'llm_tooling', 'mlops', 'data_stack', 'compute',
        'safety', 'hub', 'community', 'learning',
        -- Ops (this migration).
        'iac',              -- Terraform, Pulumi, Ansible
        'containers',       -- Docker, Podman, BuildKit
        'orchestration',    -- Kubernetes, Nomad
        'cicd',             -- Actions, GitLab CI, Argo, Tekton
        'observability',    -- Prometheus, Grafana, OpenTelemetry
        'database',         -- PostgreSQL, ClickHouse, and the tooling around
        'secrets',          -- Vault, SOPS
        'cloud_free_tier'   -- Where a person with no budget can practise
    ));

COMMENT ON TABLE external_resources IS
    'Curated tooling, communities and courses, per domain. Curated because '
    'the value is in the selection: a link dump is what somebody already '
    'failed to navigate before arriving here.';

COMMENT ON COLUMN external_resources.domain IS
    'Which domain''s toolkit this belongs to. A row rather than a table per '
    'domain: the curation workflow is the same one either way.';

CREATE INDEX idx_external_resources_domain
    ON external_resources (domain, category, sort_order)
    WHERE is_curated = TRUE;

-- ═══════════════════════════════════════════════════════════════════
-- The ops toolkit
-- ═══════════════════════════════════════════════════════════════════
--
-- Ordered by what somebody meets first rather than by what is most powerful.
-- Somebody arriving in this domain needs Docker and a pipeline before they
-- need a service mesh, and a list that opens on Kubernetes teaches them that
-- the trade is out of reach.
--
-- Where a tool has a paid-only version, the note says so. In a domain where
-- the licensing of the observability stack is a live cost question, hiding it
-- would be the same failure as pretending a GPU is optional.

INSERT INTO external_resources
    (slug, display_name, domain, category, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES

-- ── Infrastructure as code ─────────────────────────────────────────
('terraform', 'Terraform', 'ops', 'iac', 'https://developer.hashicorp.com/terraform',
 'La description d''une infrastructure comme du code, et l''outil que la plupart des offres d''emploi nomment.',
 'Gratuit en local. La licence n''est plus libre depuis 2023 : pour un usage commercial, lire la BUSL avant de s''engager.',
 ARRAY['devops-engineer','cloud-architect','platform-engineer'], 10),

('opentofu', 'OpenTofu', 'ops', 'iac', 'https://opentofu.org',
 'L''embranchement communautaire de Terraform, sous licence libre. Compatible avec la plupart des configurations existantes.',
 'Libre, fondation Linux. Le choix par défaut si la licence compte.',
 ARRAY['devops-engineer','cloud-architect'], 20),

('pulumi', 'Pulumi', 'ops', 'iac', 'https://www.pulumi.com',
 'La même chose en langage de programmation réel. Utile quand la logique de déploiement est vraiment de la logique.',
 'Gratuit pour une personne seule ; l''état partagé en équipe est payant, ou auto-hébergé.',
 ARRAY['devops-engineer','platform-engineer'], 30),

('ansible', 'Ansible', 'ops', 'iac', 'https://docs.ansible.com',
 'Configuration sans agent, par SSH. Ce qu''on utilise quand il y a des machines et pas un nuage.',
 'Libre. Rien à installer sur les machines cibles, ce qui en fait le plus abordable des quatre.',
 ARRAY['devops-engineer'], 40),

-- ── Conteneurs ─────────────────────────────────────────────────────
('docker', 'Docker', 'ops', 'containers', 'https://docs.docker.com',
 'Le format d''image que tout le reste consomme. À apprendre en premier, avant toute orchestration.',
 'Moteur libre. Docker Desktop est payant au-delà d''une certaine taille d''entreprise ; sous Linux il n''est pas nécessaire.',
 ARRAY['devops-engineer','platform-engineer','kubernetes-specialist'], 50),

('podman', 'Podman', 'ops', 'containers', 'https://podman.io',
 'Des conteneurs sans démon privilégié. Même ligne de commande, moins de surface d''attaque.',
 'Libre. L''alternative quand la politique de sécurité refuse un démon root.',
 ARRAY['devops-engineer'], 60),

('buildkit', 'BuildKit', 'ops', 'containers', 'https://github.com/moby/buildkit',
 'La construction d''images, en parallèle et avec un cache qui sert vraiment. Ce qui transforme un pipeline de dix minutes en un de deux.',
 'Libre, intégré à Docker récent.',
 ARRAY['devops-engineer','platform-engineer'], 70),

-- ── Orchestration ──────────────────────────────────────────────────
('kubernetes', 'Kubernetes', 'ops', 'orchestration', 'https://kubernetes.io/docs',
 'L''orchestrateur de fait. Grand, et rentable seulement au-delà d''une certaine taille — le dire fait partie du métier.',
 'Libre. S''apprend en local avec kind ou k3s, sans un centime de nuage.',
 ARRAY['kubernetes-specialist','platform-engineer','devops-engineer'], 80),

('k3s', 'k3s', 'ops', 'orchestration', 'https://k3s.io',
 'Un Kubernetes complet en un binaire. Le meilleur endroit pour apprendre : il tourne sur une machine modeste.',
 'Libre. Tourne sur un Raspberry Pi, ce qui en fait le terrain d''apprentissage le moins cher du domaine.',
 ARRAY['kubernetes-specialist'], 90),

('nomad', 'Nomad', 'ops', 'orchestration', 'https://developer.hashicorp.com/nomad',
 'Plus simple que Kubernetes, et suffisant pour beaucoup de charges. La comparaison honnête vaut d''être faite avant de choisir.',
 'Gratuit en local, licence BUSL comme Terraform.',
 ARRAY['platform-engineer'], 100),

-- ── Intégration et livraison ───────────────────────────────────────
('github-actions', 'GitHub Actions', 'ops', 'cicd', 'https://docs.github.com/actions',
 'Le pipeline le plus rapide à mettre en place quand le code est déjà sur GitHub.',
 'Gratuit sur dépôt public, et généreux sur dépôt privé. C''est là qu''on écrit son premier pipeline.',
 ARRAY['devops-engineer'], 110),

('gitlab-ci', 'GitLab CI', 'ops', 'cicd', 'https://docs.gitlab.com/ee/ci/',
 'Intégré au dépôt, avec des exécuteurs qu''on peut héberger soi-même.',
 'Palier gratuit avec des minutes limitées ; un exécuteur auto-hébergé les rend illimitées.',
 ARRAY['devops-engineer'], 120),

('argo-cd', 'Argo CD', 'ops', 'cicd', 'https://argo-cd.readthedocs.io',
 'GitOps : le cluster se rapproche tout seul de ce qui est écrit dans le dépôt. Le déploiement cesse d''être un geste.',
 'Libre. Suppose un cluster, donc k3s en local d''abord.',
 ARRAY['kubernetes-specialist','platform-engineer'], 130),

('tekton', 'Tekton', 'ops', 'cicd', 'https://tekton.dev',
 'Des pipelines décrits comme des objets Kubernetes. Cohérent si tout le reste l''est déjà.',
 'Libre.',
 ARRAY['kubernetes-specialist'], 140),

-- ── Observabilité ──────────────────────────────────────────────────
('prometheus', 'Prometheus', 'ops', 'observability', 'https://prometheus.io/docs',
 'Les métriques, et le langage de requête que tout le domaine connaît.',
 'Libre. Tourne sur une petite machine ; la cardinalité, pas le logiciel, est ce qui coûte.',
 ARRAY['observability-engineer','sre'], 150),

('grafana', 'Grafana', 'ops', 'observability', 'https://grafana.com/docs',
 'Ce que les gens regardent pendant un incident. Un tableau de bord qui répond à une question vaut mieux qu''un mur de courbes.',
 'Libre en auto-hébergement. L''offre infogérée a un palier gratuit réel.',
 ARRAY['observability-engineer','sre'], 160),

('loki', 'Loki', 'ops', 'observability', 'https://grafana.com/docs/loki/',
 'Les journaux, indexés par étiquette plutôt que par contenu. Beaucoup moins cher que l''alternative habituelle.',
 'Libre.',
 ARRAY['observability-engineer'], 170),

('tempo', 'Tempo', 'ops', 'observability', 'https://grafana.com/docs/tempo/',
 'Les traces, stockées sur objet. Le maillon qui relie une requête lente à la ligne qui la ralentit.',
 'Libre.',
 ARRAY['observability-engineer'], 180),

('opentelemetry', 'OpenTelemetry', 'ops', 'observability', 'https://opentelemetry.io/docs/',
 'La norme d''instrumentation, indépendante du fournisseur. Instrumenter une fois, changer d''outil ensuite sans réécrire.',
 'Libre. À apprendre avant n''importe quel agent propriétaire : c''est ce qui rend le changement possible.',
 ARRAY['observability-engineer','sre','platform-engineer'], 190),

('jaeger', 'Jaeger', 'ops', 'observability', 'https://www.jaegertracing.io',
 'Traçage distribué, avec une interface lisible pour suivre une requête à travers les services.',
 'Libre.',
 ARRAY['observability-engineer'], 200),

-- ── Bases de données ───────────────────────────────────────────────
('postgresql', 'PostgreSQL', 'ops', 'database', 'https://www.postgresql.org/docs/',
 'Le moteur par défaut, et celui sur lequel Skilluv tourne. La documentation est la meilleure du domaine.',
 'Libre. La documentation seule est une formation complète.',
 ARRAY['database-administrator','devops-engineer'], 210),

('pgbouncer', 'PgBouncer', 'ops', 'database', 'https://www.pgbouncer.org',
 'Le regroupement de connexions, c''est-à-dire ce qui manque à presque toute application qui sature sa base.',
 'Libre.',
 ARRAY['database-administrator'], 220),

('clickhouse', 'ClickHouse', 'ops', 'database', 'https://clickhouse.com/docs',
 'Analytique en colonnes. Une agrégation sur un milliard de lignes en une seconde, si le schéma est bien posé.',
 'Libre en auto-hébergement.',
 ARRAY['database-administrator'], 230),

('pgbackrest', 'pgBackRest', 'ops', 'database', 'https://pgbackrest.org',
 'Les sauvegardes et la restauration à un instant donné. La partie du métier qui ne se découvre pas le jour de la panne.',
 'Libre.',
 ARRAY['database-administrator','sre'], 240),

-- ── Secrets ────────────────────────────────────────────────────────
('vault', 'HashiCorp Vault', 'ops', 'secrets', 'https://developer.hashicorp.com/vault',
 'Le magasin de secrets de référence, avec des identifiants à durée de vie courte.',
 'Version libre disponible ; licence BUSL pour l''usage commercial.',
 ARRAY['devops-engineer','platform-engineer'], 250),

('sops', 'SOPS', 'ops', 'secrets', 'https://github.com/getsops/sops',
 'Chiffrer les secrets dans le dépôt lui-même. La solution la plus simple qui ne soit pas un secret en clair.',
 'Libre. Le premier pas raisonnable quand Vault est trop lourd.',
 ARRAY['devops-engineer'], 260),

('external-secrets', 'External Secrets Operator', 'ops', 'secrets', 'https://external-secrets.io',
 'Le pont entre un magasin de secrets et les objets d''un cluster, sans copier-coller.',
 'Libre.',
 ARRAY['kubernetes-specialist'], 270),

-- ── Où pratiquer sans budget ───────────────────────────────────────
--
-- The category the domain needs most and the one nobody publishes honestly.
-- Every note here says what happens when the tier ends, because the
-- alternative is somebody discovering it on an invoice.
('aws-free-tier', 'AWS Free Tier', 'ops', 'cloud_free_tier', 'https://aws.amazon.com/free/',
 'Douze mois de petites instances, plus une poignée de services gratuits en permanence.',
 'Carte bancaire exigée à l''inscription. Au-delà du palier, la facturation est automatique et sans avertissement : poser une alerte de budget à un euro avant toute autre chose.',
 ARRAY['cloud-architect','devops-engineer'], 300),

('gcp-free-tier', 'Google Cloud Always Free', 'ops', 'cloud_free_tier', 'https://cloud.google.com/free',
 'Une petite machine et quelques services gratuits sans limite de durée, plus un crédit de départ.',
 'Carte exigée. Le palier permanent est modeste mais réellement permanent, ce qui en fait le meilleur endroit pour laisser tourner un projet d''apprentissage.',
 ARRAY['cloud-architect'], 310),

('azure-free', 'Azure Free Account', 'ops', 'cloud_free_tier', 'https://azure.microsoft.com/free/',
 'Un crédit de départ et des services gratuits douze mois.',
 'Carte exigée. Le compte peut être maintenu en mode gratuit à l''échéance, mais il faut le demander explicitement.',
 ARRAY['cloud-architect'], 320),

('oracle-free', 'Oracle Cloud Free Tier', 'ops', 'cloud_free_tier', 'https://www.oracle.com/cloud/free/',
 'Le palier permanent le plus généreux du marché : plusieurs cœurs ARM et de la mémoire, sans limite de durée.',
 'Le meilleur rapport pour apprendre Kubernetes sans budget. Les instances gratuites peuvent être récupérées si elles restent inactives : ce n''est pas de l''hébergement de production.',
 ARRAY['cloud-architect','kubernetes-specialist'], 330),

('cloudflare-workers-free', 'Cloudflare Workers', 'ops', 'cloud_free_tier', 'https://developers.cloudflare.com/workers/',
 'Cent mille requêtes par jour en périphérie, sans carte bancaire.',
 'Pas de carte demandée, ce qui en fait souvent le seul palier réellement accessible depuis certains pays.',
 ARRAY['platform-engineer','devops-engineer'], 340),

('fly-io-free', 'Fly.io', 'ops', 'cloud_free_tier', 'https://fly.io/docs/',
 'Déployer un conteneur dans plusieurs régions avec une seule commande.',
 'Carte exigée pour la vérification. Les petites machines coûtent quelques euros par mois : ce n''est pas gratuit, c''est peu cher, et la différence se dit.',
 ARRAY['devops-engineer'], 350)

ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Communities and reading
-- ═══════════════════════════════════════════════════════════════════
--
-- Two books do most of the teaching in this domain, and both are free to
-- read. Somebody who has read them and practised on a free tier is further
-- along than somebody who has bought a certification.

INSERT INTO external_resources
    (slug, display_name, domain, category, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
('google-sre-book', 'Google SRE Book', 'ops', 'learning', 'https://sre.google/books/',
 'Le livre qui a nommé le métier : objectifs de service, budgets d''erreur, astreinte tenable.',
 'Lisible en ligne gratuitement, en entier. Le point de départ du métier SRE.',
 ARRAY['sre','incident-commander'], 400),

('observability-engineering', 'Observability Engineering', 'ops', 'learning', 'https://info.honeycomb.io/observability-engineering-oreilly-book-2022',
 'Ce que l''observabilité veut dire quand on ne peut plus prévoir les questions à l''avance.',
 'Version électronique gratuite contre une adresse électronique.',
 ARRAY['observability-engineer'], 410),

('cncf-landscape', 'CNCF Landscape', 'ops', 'community', 'https://landscape.cncf.io',
 'La carte de l''écosystème, utile surtout pour situer un outil qu''on vient d''entendre nommer.',
 'Libre. À lire comme une carte, pas comme une liste de courses.',
 ARRAY['kubernetes-specialist','cloud-architect'], 420),

('postgres-weekly', 'Postgres Weekly', 'ops', 'community', 'https://postgresweekly.com',
 'Une lettre hebdomadaire qui suit ce qui bouge dans le moteur et autour.',
 'Gratuit.',
 ARRAY['database-administrator'], 430)

ON CONFLICT (slug) DO NOTHING;
