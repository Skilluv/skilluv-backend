-- Phase P0 — Fondations du modèle cible.
-- Migration 0056 : création de la table `skill_nodes` (skill graph atomique).
--
-- Rationale :
--   Le modèle actuel `skill_fragments (skill_domain, sub_skill)` est trop pauvre
--   pour dire "Marie sait react-hooks, postgresql-indexing, git-rebase-interactive".
--   `skill_nodes` est un graphe hiérarchique de compétences atomiques (chacune un
--   "geste" mesurable/prouvable) qui sert de vocabulaire commun pour :
--     - Tagger les project_slices (via slice_skills, migration 0059)
--     - Prouver les compétences user (via user_skills, migration 0060)
--     - Déclencher les attestations gesture / skill / compagnonnage (Phase P5)
--
-- Structure :
--   - Hiérarchie 2 niveaux via `parent_id` (catégorie → skill atomique)
--   - `slug` UNIQUE en kebab-case anglais (stable, machine-readable)
--   - `display_name` FR/EN (l'i18n complète viendra plus tard via rust-i18n)
--   - `aliases TEXT[]` : synonymes pour la recherche fuzzy (ex: 'useState' pour react-hooks)
--   - `external_refs JSONB` : liens vers MDN, Wikipedia, RFC, docs officielles
--
-- Seed : voir migration 0057 (script YAML → SQL depuis docs/skill-nodes-seed.yaml).

CREATE TABLE skill_nodes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(80) NOT NULL UNIQUE,
    display_name VARCHAR(150) NOT NULL,
    description TEXT,
    domain VARCHAR(30) NOT NULL
        CHECK (domain IN (
            'code',           -- programming languages, frameworks, backend, DBs
            'design',         -- UI/UX, design systems, Figma/Penpot
            'game',           -- Godot, gameplay programming, 2D/3D craft
            'security',       -- web app sec, auth, crypto, offensive, reporting
            'soft_skills',    -- communication, review, mentoring, leadership
            'ai',             -- prompt engineering, LLM integration, AI collab
            'ops'             -- git, CI/CD, containers, observability, incident
        )),
    parent_id UUID REFERENCES skill_nodes(id) ON DELETE RESTRICT,
    -- Alias de recherche (ex: 'useState', 'react-state-hooks' pour 'react-hooks')
    aliases TEXT[] NOT NULL DEFAULT '{}',
    -- Références externes ({"mdn": "https://...", "wikipedia": "..."})
    external_refs JSONB,
    -- Marqué "skilluv-specific" = pertinent pour contributeurs Skilluv ou contexte africain
    -- (mobile money, tenant multi, Rust/Axum stack, VAPID natif, etc.)
    is_skilluv_specific BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Contrainte : un skill ne peut pas être son propre parent
    CONSTRAINT skill_nodes_no_self_parent CHECK (id != parent_id)
);

-- Recherche par domaine (utilisé partout pour filtrer skills par catégorie Skilluv)
CREATE INDEX idx_skill_nodes_domain
    ON skill_nodes (domain);

-- Recherche par parent (afficher les enfants d'une catégorie)
CREATE INDEX idx_skill_nodes_parent
    ON skill_nodes (parent_id)
    WHERE parent_id IS NOT NULL;

-- Recherche fuzzy par alias (utilisé pour "quel skill match ce mot-clé ?")
CREATE INDEX idx_skill_nodes_aliases
    ON skill_nodes USING gin (aliases);

-- Filtrage rapide des skills Skilluv-specific (pour onboarding contributeur)
CREATE INDEX idx_skill_nodes_skilluv_specific
    ON skill_nodes (is_skilluv_specific)
    WHERE is_skilluv_specific = TRUE;

-- Trigger classique de maintenance de updated_at
CREATE OR REPLACE FUNCTION touch_skill_nodes_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER skill_nodes_updated_at
    BEFORE UPDATE ON skill_nodes
    FOR EACH ROW
    EXECUTE FUNCTION touch_skill_nodes_updated_at();
