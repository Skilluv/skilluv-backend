-- Phase P0 — Fondations du modèle cible challenges/slices/deliverables.
-- Migration 0055 : extensions non-breaking de la table `projects` pour supporter
-- le nouveau modèle (voir `docs/challenges-target-model-and-roadmap.md` section B.2).
--
-- Rationale :
--   1. `skill_domains TEXT[]` — Un projet peut désormais couvrir plusieurs domaines
--      Skilluv (code + design + sec + game). Résout l'isolation actuelle du game
--      dev (voir section 9.3 du doc vision-and-gap-analysis).
--   2. `lifecycle_status` — Modélise le lifecycle projet incubating → active →
--      mature → graduated → archived (aligné section 9.4 vision doc, "seasons à
--      deux horloges").
--   3. `figma_url` + `github_repo_owner/name` — Rattachement explicite aux
--      artefacts externes qui produiront des slices (Phase P1).
--   4. `bug_bounty_open` + `bug_bounty_scope` — Consentement projet aux guerres
--      de territoire sec ↔ code (section 9.2 vision doc).
--   5. `slice_ingestion_mode` — Détermine si les issues GitHub curées deviennent
--      automatiquement des slices (`auto`), passent par review du steward
--      (`curator_review`, défaut), ou nécessitent une création manuelle uniquement
--      (`manual_only`). Décision Q8 (session 2026-07-09).
--   6. Métriques agrégées (`active_contributor_count`, `merged_deliverable_count`,
--      `health_score`) — dénormalisation contrôlée pour perf du dashboard santé
--      projet et calcul de fin de saison. Recalculées via job async, jamais lues
--      pour prises de décision critiques.
--
-- Non-breaking : toutes les colonnes ajoutées ont un DEFAULT ou sont NULLables.
-- Le code existant continue de fonctionner sans lire ces colonnes.

ALTER TABLE projects
    ADD COLUMN skill_domains TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN lifecycle_status VARCHAR(20) NOT NULL DEFAULT 'incubating'
        CHECK (lifecycle_status IN (
            'incubating',   -- projet vient d'être curated, pas encore de contribs
            'active',       -- contribs régulières
            'mature',       -- projet stable, capstone-worthy
            'graduated',    -- projet shippé, vit sa vie hors Skilluv (immortal contributors)
            'archived'      -- projet abandonné ou clôturé
        )),
    ADD COLUMN figma_url VARCHAR(500),
    ADD COLUMN github_repo_owner VARCHAR(120),
    ADD COLUMN github_repo_name VARCHAR(200),
    ADD COLUMN bug_bounty_open BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN bug_bounty_scope JSONB,
    ADD COLUMN slice_ingestion_mode VARCHAR(20) NOT NULL DEFAULT 'curator_review'
        CHECK (slice_ingestion_mode IN (
            'auto',              -- ingestion automatique via webhook GitHub, publish direct
            'curator_review',    -- ingestion crée un draft, steward valide avant publish
            'manual_only'        -- pas d'ingestion auto, tout est créé à la main
        )),
    ADD COLUMN active_contributor_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN merged_deliverable_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN health_score DECIMAL(3,2)
        CHECK (health_score IS NULL OR (health_score >= 0.0 AND health_score <= 1.0));

-- Index pour filtrage par lifecycle sur les vues publiques
CREATE INDEX idx_projects_lifecycle
    ON projects (lifecycle_status)
    WHERE archived_at IS NULL;

-- Index GIN sur skill_domains pour recherche multi-domaine ("projets qui touchent au code ET design")
CREATE INDEX idx_projects_skill_domains
    ON projects USING gin (skill_domains);

-- Index pour retrouver rapidement le projet depuis un repo GitHub (utilisé par le webhook)
CREATE INDEX idx_projects_github_repo
    ON projects (github_repo_owner, github_repo_name)
    WHERE github_repo_owner IS NOT NULL AND github_repo_name IS NOT NULL;

-- Backfill léger : les projets existants avec repo_url pointant vers github.com
-- reçoivent leur owner/name renseigné pour ne pas casser le webhook.
-- On parse `https://github.com/{owner}/{name}` en best-effort ; les URLs
-- non-standard restent NULL et devront être renseignées manuellement par un steward.
UPDATE projects
SET
    github_repo_owner = split_part(
        regexp_replace(repo_url, '^https?://(www\.)?github\.com/', ''),
        '/', 1
    ),
    github_repo_name = split_part(
        regexp_replace(
            regexp_replace(repo_url, '^https?://(www\.)?github\.com/', ''),
            '\.git$', ''
        ),
        '/', 2
    )
WHERE repo_url ~* '^https?://(www\.)?github\.com/[^/]+/[^/]+/?$'
  AND github_repo_owner IS NULL;

-- Note : le backfill de `skill_domains` depuis `tech_stack` est laissé pour un
-- script métier ultérieur (les valeurs de tech_stack sont libres, pas mappables
-- automatiquement vers les domaines Skilluv). Les projets existants restent avec
-- skill_domains = '{}' jusqu'à ce qu'un admin/steward les mette à jour.
