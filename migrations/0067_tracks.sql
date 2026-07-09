-- Phase P3 — Tracks (curriculums structurants).
-- Migration 0067 : tables `tracks`, `track_challenges`, `user_tracks` + seed initial.
--
-- Rationale (voir docs/challenges-target-model-and-roadmap.md sections B.11 et 5.5) :
--   Un track incarne les 4 phases de la vision produit (bootstrap → katas →
--   contribs → impact). Le user s'enrolle dans un track à l'onboarding
--   ("tu veux devenir quoi ?"), et voit sa progression jusqu'à la graduation.
--
--   `track_challenges` ordonne les challenges dans un track (position UNIQUE).
--   `user_tracks` est le journal d'enrollment + progression (current_challenge_id).

-- ═══════════════════════════════════════════════════════════════════
-- tracks
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE tracks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(80) NOT NULL UNIQUE,       -- 'frontend-foundations'
    name VARCHAR(150) NOT NULL,
    description TEXT NOT NULL,
    target_domain VARCHAR(30) NOT NULL
        CHECK (target_domain IN ('code','design','game','security','soft_skills','ai','ops')),
    target_phase VARCHAR(20) NOT NULL
        CHECK (target_phase IN ('bootstrap','katas','contribs','impact')),
    estimated_hours INTEGER CHECK (estimated_hours IS NULL OR estimated_hours > 0),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tracks_domain_phase
    ON tracks (target_domain, target_phase)
    WHERE active = TRUE;

CREATE OR REPLACE FUNCTION touch_tracks_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tracks_updated_at
    BEFORE UPDATE ON tracks
    FOR EACH ROW
    EXECUTE FUNCTION touch_tracks_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- track_challenges (M2M ordonnée)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE track_challenges (
    track_id UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    challenge_id UUID NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    -- false = étape obligatoire du track, true = étape recommandée (le user
    -- peut la skip sans bloquer la progression)
    optional BOOLEAN NOT NULL DEFAULT FALSE,

    PRIMARY KEY (track_id, challenge_id),

    -- Position UNIQUE dans un track (pas de collision d'ordre)
    UNIQUE (track_id, position)
);

CREATE INDEX idx_track_challenges_by_track
    ON track_challenges (track_id, position);

CREATE INDEX idx_track_challenges_by_challenge
    ON track_challenges (challenge_id);

-- ═══════════════════════════════════════════════════════════════════
-- user_tracks (enrollment + progression)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE user_tracks (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    -- Le prochain challenge suggéré au user (peut être NULL si track fini
    -- ou si le user n'a pas encore commencé)
    current_challenge_id UUID REFERENCES challenges(id) ON DELETE SET NULL,

    PRIMARY KEY (user_id, track_id)
);

CREATE INDEX idx_user_tracks_by_user
    ON user_tracks (user_id)
    WHERE completed_at IS NULL;

CREATE INDEX idx_user_tracks_completed
    ON user_tracks (user_id, completed_at DESC)
    WHERE completed_at IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Seed initial : 5 tracks Foundations (un par domaine "core produit")
-- ═══════════════════════════════════════════════════════════════════
-- Les track_challenges seront ajoutés par l'admin quand les challenges
-- correspondants existeront. Un track sans challenges est un placeholder
-- honnête : l'inscription enregistre l'intention.

INSERT INTO tracks (slug, name, description, target_domain, target_phase, estimated_hours)
VALUES
    (
        'frontend-foundations',
        'Frontend Foundations',
        'Les gestes essentiels du web moderne : HTML sémantique, CSS layout, JS core, TypeScript. À la sortie de ce track, tu peux contribuer sur un projet SvelteKit ou React réel.',
        'code',
        'bootstrap',
        60
    ),
    (
        'backend-foundations',
        'Backend Foundations',
        'API design, bases de données, tests, sécurité. À la sortie, tu peux contribuer sur une API Rust (Axum) ou Python (FastAPI) réelle.',
        'code',
        'bootstrap',
        80
    ),
    (
        'security-foundations',
        'Security Foundations',
        'OWASP Top 10, authentification, cryptographie basique, rapport de vulnérabilité. À la sortie, tu peux soumettre ton premier CVE.',
        'security',
        'bootstrap',
        50
    ),
    (
        'design-foundations',
        'Design Foundations',
        'Typographie, layout, design tokens, Figma craft, accessibilité. À la sortie, tu peux livrer un composant dans un design system OSS.',
        'design',
        'bootstrap',
        40
    ),
    (
        'game-foundations',
        'Game Foundations',
        'Godot fundamentals, gameplay programming, 2D craft. À la sortie, tu peux publier un jeu prototype jouable en 30 minutes.',
        'game',
        'bootstrap',
        50
    );
