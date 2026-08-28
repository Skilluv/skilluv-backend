-- Priorite moyenne #4 strategy doc §15 : activer l'ingestion GitHub sur les
-- 12 partenaires OSS (Annexe F). Le catalogue de "100+ micro-quetes" est
-- DYNAMIQUE — populate par le worker `bin/github_ingest.rs` qui lit les
-- issues open avec au moins un des `curated_labels` sur chaque projet
-- ayant `github_repo_owner` + `github_repo_name` + `slice_ingestion_mode`
-- != 'manual_only'.
--
-- Cette migration/seed configure ces 3 colonnes pour les 12 partenaires
-- seedes par `seed_oss_partners.sql`. Idempotent : UPDATE naturellement.
--
-- Modes d'ingestion :
--   - `curator_review` : le slice est cree en `draft`, steward valide via
--     P11.4 avant qu'il apparaisse au catalogue public.
--   - `auto` : publication directe. Reserve aux projets ou on a l'engagement
--     du mainteneur (Tier 2/3) et une confiance eprouvee dans le labeling.
--
-- Labels curates : GitHub `?labels=X,Y,Z` = OR logique. On garde les 3
-- classiques + eventuellement un label projet-specifique.

-- ── Fullstack Rust / DB ────────────────────────────────────────────
UPDATE projects SET
    github_repo_owner = 'launchbadge',
    github_repo_name = 'sqlx',
    curated_labels = ARRAY['good-first-issue','help-wanted','E-easy'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'sqlx';

UPDATE projects SET
    github_repo_owner = 'tokio-rs',
    github_repo_name = 'axum',
    curated_labels = ARRAY['good first issue','help wanted','E-easy'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'axum';

UPDATE projects SET
    github_repo_owner = 'longbridge',
    github_repo_name = 'rust-i18n',
    curated_labels = ARRAY['good first issue','help wanted'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'rust-i18n';

UPDATE projects SET
    github_repo_owner = 'meilisearch',
    github_repo_name = 'meilisearch',
    curated_labels = ARRAY['good first issue','help wanted','documentation'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'meilisearch';

UPDATE projects SET
    github_repo_owner = 'bevyengine',
    github_repo_name = 'bevy',
    curated_labels = ARRAY['D-Good-First-Issue','C-Docs','A-ECS'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'bevy';

-- ── Frontend / TS ──────────────────────────────────────────────────
UPDATE projects SET
    github_repo_owner = 'calcom',
    github_repo_name = 'cal.com',
    curated_labels = ARRAY['good first issue','help wanted','\u{1F914} needs discussion'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'calcom';

UPDATE projects SET
    github_repo_owner = 'excalidraw',
    github_repo_name = 'excalidraw',
    curated_labels = ARRAY['good first issue','help wanted','a11y'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'excalidraw';

UPDATE projects SET
    github_repo_owner = 'directus',
    github_repo_name = 'directus',
    curated_labels = ARRAY['good first issue','help wanted','documentation'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'directus';

UPDATE projects SET
    github_repo_owner = 'nestjs',
    github_repo_name = 'nest',
    curated_labels = ARRAY['good first issue','help wanted'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'nestjs';

UPDATE projects SET
    github_repo_owner = 'prisma',
    github_repo_name = 'prisma',
    curated_labels = ARRAY['good first issue','help wanted','kind/docs'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'prisma';

-- ── Mobile ─────────────────────────────────────────────────────────
UPDATE projects SET
    github_repo_owner = 'flutter',
    github_repo_name = 'flutter',
    curated_labels = ARRAY['good first issue','help wanted','d: examples'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'flutter';

-- ── DevOps ────────────────────────────────────────────────────────
UPDATE projects SET
    github_repo_owner = 'coollabsio',
    github_repo_name = 'coolify',
    curated_labels = ARRAY['good first issue','help wanted','documentation'],
    slice_ingestion_mode = 'curator_review'
WHERE slug = 'coolify';

-- Recap
SELECT
    slug,
    github_repo_owner || '/' || github_repo_name AS repo,
    array_length(curated_labels, 1) AS labels_count,
    slice_ingestion_mode
FROM projects
WHERE curated_by_admin = true
  AND github_repo_owner IS NOT NULL
ORDER BY slug;
