-- Phase P17.6 — Events + participation (Hacktoberfest, Skilluv Fest, saisons).
-- Migration 0093.
--
-- Rationale :
--   La spec UX badges définit les EVENT STAMPS (tampons hexagonaux) émis
--   pour la participation à des événements datés : Hacktoberfest, Skilluv
--   Fest, hackathons partenaires, saisons Skilluv.
--
--   Table minimale pour :
--     - Créer un event (admin uniquement)
--     - Enregistrer la participation d'un user (self-serve ou attribution admin)
--     - Générer un event_stamp via badge_rules (P17.3 rules engine, en
--       étendant le proof_type "event_participation")
--
--   Le stamp reste à vie sur le profil (souvenir historique) — même si
--   l'event est passé.

CREATE TABLE events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(60) NOT NULL UNIQUE
        CHECK (slug ~ '^[a-z0-9-]+$' AND length(slug) BETWEEN 3 AND 60),
    name VARCHAR(120) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    -- Fenêtre temporelle : starts_at obligatoire, ends_at optionnel
    -- (event ouvert type Hacktoberfest annuel).
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ,
    -- Thème visuel du stamp (couleur, symbole) — consommé par le frontend.
    visual_theme JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- true = événement partenaire (badge unique à leur charte)
    is_partner BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT events_end_after_start
        CHECK (ends_at IS NULL OR ends_at >= starts_at)
);

CREATE INDEX idx_events_active_starts ON events (is_active, starts_at DESC);

-- Participation : un user peut rejoindre 1 event (idempotent).
-- L'attribution du stamp lui-même passe par le badge_engine (P17.3) qui
-- consomme cette table comme "proof" d'un output_type='event_stamp'.
CREATE TABLE user_event_participation (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_id UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Optionnel : lien vers l'artefact spécifique produit lors de l'event
    -- (PR mergée pendant Hacktoberfest, projet livré pendant Skilluv Fest).
    contribution_ref VARCHAR(500),
    notes TEXT,
    PRIMARY KEY (user_id, event_id)
);

CREATE INDEX idx_uep_by_event ON user_event_participation (event_id, joined_at DESC);
