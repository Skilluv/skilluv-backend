-- Phase P16.1 — Orientations métier + mapping vers skills.
-- Migration 0088.
--
-- Rationale :
--   Aujourd'hui `challenge_templates.skill_domain` est mono-valué (7 domaines
--   grossiers : code/design/game/security/ai/ops/soft_skills). Résultat produit :
--   silos artificiels — un "codeur" ne peut pas faire de jeu, un "designer" ne
--   fait pas de sécurité. La DB permet déjà le multi-tag (`slice_skills`) mais
--   l'UX force le mono-domaine.
--
--   Ce refactor introduit une couche intermédiaire "orientation" (métier) :
--
--     domain (macro, interne)
--        └── orientation (métier, ~30 curated)   ← ce que l'user choisit
--              └── skills (atomique, prouvé par artefact)
--
--   Distinction claire avec `tracks` (P3, mig 0067) :
--     - track      = curriculum ordonné de challenges (parcours pédagogique)
--     - orientation = métier / job title (dev-frontend, pentester-web…)
--   Un user peut avoir 3 orientations et suivre 2 tracks à la fois — c'est
--   deux axes indépendants.
--
--   Une orientation :
--     - a un slug unique (dev-frontend, game-sound-engineer…)
--     - a un `primary_domain` (référence macro)
--     - peut avoir plusieurs `secondary_domains` (multi-disciplinaire)
--     - a un catalogue de skills recommandés/core via `orientation_skill_map`
--
--   `is_curated` distingue les orientations officielles (admin-owned) des
--   propositions communauté en review (évite l'inflation de slugs quasi-dupliqués).

CREATE TABLE orientations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(60) NOT NULL UNIQUE
        CHECK (slug ~ '^[a-z0-9-]+$' AND length(slug) BETWEEN 3 AND 60),
    name VARCHAR(120) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    primary_domain VARCHAR(30) NOT NULL
        CHECK (primary_domain IN (
            'code', 'design', 'game', 'security', 'soft_skills', 'ai', 'ops'
        )),
    secondary_domains TEXT[] NOT NULL DEFAULT '{}',
    tags TEXT[] NOT NULL DEFAULT '{}',           -- audio, blockchain, mobile, web…
    -- Curation : true = catalogue officiel, false = proposé communauté (en review).
    is_curated BOOLEAN NOT NULL DEFAULT FALSE,
    -- Anti-inflation : orientations archivées ne peuvent plus être choisies à
    -- l'inscription mais restent visibles sur les profils historiques.
    is_archived BOOLEAN NOT NULL DEFAULT FALSE,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_orientations_domain_curated
    ON orientations (primary_domain, is_curated, is_archived);

CREATE INDEX idx_orientations_tags_gin
    ON orientations USING gin (tags);

-- Mapping orientation ↔ skills (many-to-many pondéré).
-- - is_core=TRUE : skill INDISPENSABLE (un dev-frontend sans HTML ça n'existe pas)
-- - is_recommended=TRUE : skill fortement suggéré mais non bloquant
-- - weight : poids pour classement recruteur (proximité skill à l'orientation)
CREATE TABLE orientation_skill_map (
    orientation_id UUID NOT NULL REFERENCES orientations(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES skill_nodes(id) ON DELETE CASCADE,
    is_core BOOLEAN NOT NULL DEFAULT FALSE,
    is_recommended BOOLEAN NOT NULL DEFAULT TRUE,
    weight REAL NOT NULL DEFAULT 1.0 CHECK (weight > 0),
    PRIMARY KEY (orientation_id, skill_id)
);

CREATE INDEX idx_orientation_skill_map_skill
    ON orientation_skill_map (skill_id, is_core, is_recommended);

-- ═══════════════════════════════════════════════════════════════════
-- SEED — Catalogue initial de 31 orientations curated.
-- Choix basé sur les métiers réels + wedge Skilluv (Afrique/OSS/reconversion).
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientations (slug, name, description, primary_domain, secondary_domains, tags, is_curated) VALUES
-- ── CODE ────────────────────────────────────────────────────────
('dev-frontend',       'Développeur Frontend',       'React/Vue/Svelte, TypeScript, CSS moderne. Focus expérience utilisateur web.', 'code', ARRAY['design'], ARRAY['web'], TRUE),
('dev-backend',        'Développeur Backend',        'API REST/GraphQL, DB relationnelles, auth. Rust, Go, Node, Python.',           'code', ARRAY['ops'], ARRAY['api','server'], TRUE),
('dev-fullstack',      'Développeur Fullstack',      'Front + back T-shaped. Autonome sur un projet web bout-en-bout.',              'code', ARRAY['design','ops'], ARRAY['web'], TRUE),
('mobile-android',     'Développeur Android',        'Kotlin, Jetpack Compose, écosystème Google.',                                   'code', ARRAY['design'], ARRAY['mobile'], TRUE),
('mobile-ios',         'Développeur iOS',            'Swift, SwiftUI, écosystème Apple.',                                             'code', ARRAY['design'], ARRAY['mobile'], TRUE),
('mobile-cross',       'Développeur Mobile Cross-platform', 'React Native ou Flutter — une base pour iOS + Android.',                'code', ARRAY['design'], ARRAY['mobile'], TRUE),
('systems-programmer', 'Développeur Systèmes',       'Rust, C++, bas niveau, performance, mémoire.',                                  'code', ARRAY['ops'], ARRAY['low-level'], TRUE),
('smart-contract-dev', 'Développeur Smart Contracts', 'Solidity, Cairo, contrats on-chain, sécurité DeFi.',                          'code', ARRAY['security'], ARRAY['blockchain','web3'], TRUE),

-- ── DESIGN ──────────────────────────────────────────────────────
('web-designer',       'Web Designer',               'UI/UX web, Figma, design systems, prototypes cliquables.',                     'design', ARRAY['code'], ARRAY['web'], TRUE),
('mobile-designer',    'Mobile Designer',            'UI/UX mobile natif, iOS HIG + Material Design, micro-interactions.',           'design', ARRAY['code'], ARRAY['mobile'], TRUE),
('motion-designer',    'Motion Designer',            'After Effects, Lottie, Rive. Animation d''interface et brand.',                'design', ARRAY['game'], ARRAY['animation'], TRUE),
('illustrator',        'Illustrateur Digital',       'Illustration éditoriale, character design, brand visuelle.',                   'design', ARRAY[]::TEXT[], ARRAY['art'], TRUE),
('3d-artist',          'Artiste 3D',                 'Blender, Maya. Modeling, texturing, rigging pour jeux ou film.',               'design', ARRAY['game'], ARRAY['3d'], TRUE),
('game-artist-2d',     'Artiste 2D Jeu Vidéo',       'Sprites, pixel art, animation 2D pour jeux indés.',                            'game',   ARRAY['design'], ARRAY['art','2d'], TRUE),
('game-artist-3d',     'Artiste 3D Jeu Vidéo',       'Assets 3D low-poly optimisés jeux Godot/Unity/Unreal.',                        'game',   ARRAY['design'], ARRAY['art','3d'], TRUE),

-- ── GAME ────────────────────────────────────────────────────────
('game-programmer',    'Programmeur Jeu Vidéo',      'Gameplay code, physics, AI. Godot/Unity/Unreal + C#/GDScript/C++.',            'game', ARRAY['code'], ARRAY['gameplay'], TRUE),
('game-designer',      'Game Designer',              'Game design docs, mécaniques, level design, playtest.',                        'game', ARRAY['soft_skills'], ARRAY['design'], TRUE),
('game-sound-engineer','Ingénieur Son Jeu Vidéo',   'Sound design, musique adaptative, intégration audio (FMOD, Wwise).',           'game', ARRAY['soft_skills'], ARRAY['audio'], TRUE),

-- ── AI ──────────────────────────────────────────────────────────
('data-engineer',      'Data Engineer',              'Pipelines ETL, warehouses (Snowflake, BigQuery), streaming (Kafka).',          'ai',   ARRAY['code','ops'], ARRAY['data'], TRUE),
('data-analyst',       'Data Analyst',               'SQL avancé, dashboards (Metabase, Looker), storytelling data.',                'ai',   ARRAY['soft_skills'], ARRAY['data','analytics'], TRUE),
('ml-engineer',        'ML Engineer',                'Modèles ML en prod, MLOps, expérimentation, monitoring.',                      'ai',   ARRAY['code','ops'], ARRAY['ml'], TRUE),
('prompt-engineer',    'Prompt Engineer',            'LLM prompting, agents autonomes, RAG, chaînes de tools.',                      'ai',   ARRAY['code'], ARRAY['llm'], TRUE),

-- ── OPS ─────────────────────────────────────────────────────────
('devops-engineer',    'DevOps Engineer',            'CI/CD, containers (Docker/K8s), IaC (Terraform, Ansible).',                    'ops',  ARRAY['code','security'], ARRAY['infra'], TRUE),
('sre',                'Site Reliability Engineer',  'Observabilité, SLO/SLA, incident response, chaos engineering.',                'ops',  ARRAY['code'], ARRAY['reliability'], TRUE),
('cloud-architect',    'Architecte Cloud',           'Design AWS/GCP/Azure, cost optimization, multi-region.',                       'ops',  ARRAY['code','security'], ARRAY['cloud'], TRUE),

-- ── SECURITY ────────────────────────────────────────────────────
('pentester-web',      'Pentester Web',              'OWASP Top 10, bug bounty, tests d''intrusion applicatifs web.',                'security', ARRAY['code'], ARRAY['offensive'], TRUE),
('pentester-mobile',   'Pentester Mobile',           'Sécurité apps mobiles iOS/Android, reverse engineering.',                      'security', ARRAY['code'], ARRAY['offensive','mobile'], TRUE),
('soc-analyst',        'Analyste SOC',               'SIEM, threat detection, incident response, forensics.',                        'security', ARRAY['ops'], ARRAY['defensive'], TRUE),
('security-engineer',  'Security Engineer',          'Threat modeling, secure code review, app sec by design.',                      'security', ARRAY['code'], ARRAY['defensive'], TRUE),

-- ── SHARE (soft skills valorisés) ───────────────────────────────
('tech-writer',        'Tech Writer',                'Documentation technique, tutoriels, articles, référence API.',                 'soft_skills', ARRAY['code'], ARRAY['content'], TRUE),
('open-source-maintainer','Mainteneur Open Source', 'Gouvernance projet OSS, PR reviews, community, releases.',                     'soft_skills', ARRAY['code'], ARRAY['community','oss'], TRUE);
