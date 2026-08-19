-- Slices that produce a track, a pack, a reel or a middleware project.
--
-- ## The fourth restatement of `slice_type`, and the last
--
-- 0058 wrote eight values, 0181 restated them to add `code_artifact`, 0214
-- restated them again to add `ai_artifact`. Audio would be the fourth. Same
-- shape as the domains in 0400, the capabilities in 0404 and the attestation
-- bases in 0406, same answer: a table, and a foreign key.
--
-- The type also gains something a CHECK could not carry — which domain it
-- belongs to. `figma_frame` is design, `sec_target` is security, and the
-- ingestion and the review queues have been inferring that from the slice's
-- own `primary_domain`, which is a different column that can disagree.
--
-- ## `audio_subtype` says what comes out, not where the work lives
--
-- The same split as 0181 and 0214: `slice_type` is the surface, the subtype is
-- the finished artefact.
--
-- ## Where the artefact lives is a question with two answers here
--
-- 0214 refused to host model weights, and was right: weights have free homes
-- where the people who want them already look. Audio does not have one home,
-- it has two situations.
--
--   * A finished track has somewhere to be — SoundCloud, Bandcamp, a game's
--     page — and `audio_external_hosting_url` is where a reader goes to hear
--     it in the place its author chose.
--   * A pack of thirty WAV files, a set of stems, an FMOD project, or a
--     client delivery under contract has nowhere public to be, and a review
--     needs to listen to the actual file rather than to a stream of a
--     different master.
--
-- So both exist, and neither is required by this migration: what a subtype
-- requires is stated in 0409, where the files are.
--
-- ## What is deliberately not a column here
--
-- Sample rate, bit depth, format and duration. The backlog asked for them on
-- the slice, and they cannot live there: one slice carries a master, its
-- stems and a compressed preview, which have three different answers. They
-- are per file, and 0409 is where files are.

CREATE TABLE slice_types (
    slug VARCHAR(30) PRIMARY KEY,
    -- The domain this surface belongs to. NULL for the ones that are not
    -- specific to one — a repository issue or a piece of documentation exists
    -- in every domain.
    skill_domain VARCHAR(30) REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    name VARCHAR(80) NOT NULL,
    description TEXT NOT NULL,
    sort_order SMALLINT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE slice_types IS
    'The surfaces a slice of work can live on. A table rather than a CHECK '
    'restated once per domain — 0058, 0181, 0214 — and it carries the domain '
    'each surface belongs to, which the CHECK could not.';

INSERT INTO slice_types (slug, skill_domain, name, description, sort_order) VALUES
    ('github_issue', NULL, 'Ticket amont',
     'Un ticket dans un dépôt qu''on ne contrôle pas.', 10),
    ('documentation', NULL, 'Documentation',
     'Un document à écrire ou à reprendre.', 20),
    ('other', NULL, 'Autre',
     'Ce qui n''entre dans aucune des autres surfaces.', 900),
    ('code_artifact', 'code', 'Artefact de code',
     'Une bibliothèque, un outil, un service livré.', 30),
    ('cli_task', 'code', 'Tâche en ligne de commande',
     'Un exercice résolu au terminal.', 40),
    ('figma_frame', 'design', 'Écran Figma',
     'Une maquette à produire ou à reprendre.', 50),
    ('design_token', 'design', 'Jeton de design',
     'Un élément de système de design.', 60),
    ('game_level', 'game', 'Niveau de jeu',
     'Un niveau à concevoir ou à équilibrer.', 70),
    ('game_asset', 'game', 'Ressource de jeu',
     'Un modèle, une texture, une animation.', 80),
    ('sec_target', 'security', 'Cible d''audit',
     'Un périmètre à auditer selon un protocole écrit.', 90),
    ('ai_artifact', 'ai', 'Artefact IA',
     'Un modèle, un jeu de données, un article, un service.', 100),
    ('audio_artifact', 'audio', 'Artefact audio',
     'Une composition, un pack sonore, une bande démo, un système musical, du code audio.', 110);

ALTER TABLE project_slices
    DROP CONSTRAINT IF EXISTS project_slices_slice_type_check,
    ADD CONSTRAINT project_slices_slice_type_fkey
        FOREIGN KEY (slice_type) REFERENCES slice_types(slug) ON UPDATE CASCADE;

-- ═══════════════════════════════════════════════════════════════════
-- What an audio slice says about itself
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE project_slices
    ADD COLUMN audio_subtype VARCHAR(30),
    -- What the sound is for. Not a skill domain: a composer writing for a
    -- podcast is doing audio work with a podcast destination, and filing that
    -- under a `podcast` domain would invent a trade nobody practises.
    ADD COLUMN audio_destination VARCHAR(20),
    -- Where the finished work lives publicly, when it has somewhere to live.
    ADD COLUMN audio_external_hosting_url TEXT;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_audio_subtype_values
    CHECK (audio_subtype IS NULL OR audio_subtype IN (
        'composition',            -- a finished piece, with its stems
        'sound_pack',             -- a coherent set of effects
        'voice_reel',             -- a voice actor's demonstrated range
        'adaptive_music_system',  -- a middleware project, integrated and playable
        'audio_programming',      -- DSP, spatialisation, synthesis, in code
        'ambient_soundscape'      -- long-form ambience, built to loop
    )),
    ADD CONSTRAINT project_slices_audio_destination_values
    CHECK (audio_destination IS NULL OR audio_destination IN (
        'game', 'motion', 'podcast', 'brand', 'ui', 'cross'
    )),
    -- A subtype only means something on an audio artefact, and an audio
    -- artefact without one is a slice nobody can attest against.
    ADD CONSTRAINT project_slices_audio_subtype_belongs_to_audio_artifact
    CHECK (
        (slice_type = 'audio_artifact' AND audio_subtype IS NOT NULL)
        OR (slice_type <> 'audio_artifact' AND audio_subtype IS NULL)
    );

COMMENT ON COLUMN project_slices.audio_subtype IS
    'What the finished artefact is, for audio_artifact slices. slice_type says '
    'which surface the work lives on; this says what comes out of it.';

COMMENT ON COLUMN project_slices.audio_destination IS
    'What the sound is for — a game, a montage, a podcast, a brand, an '
    'interface, or several. Deliberately not a skill domain: a composer '
    'writing for a podcast practises audio, not podcasting.';

COMMENT ON COLUMN project_slices.audio_external_hosting_url IS
    'Where the finished work lives publicly, when it has somewhere to live: a '
    'SoundCloud track, a Bandcamp release, a game page. Optional, because a '
    'pack of thirty effects and a client delivery under contract have no '
    'public home — those are files, and files are in audio_artifact_files.';

CREATE INDEX idx_project_slices_audio_subtype
    ON project_slices (audio_subtype)
    WHERE audio_subtype IS NOT NULL;

CREATE INDEX idx_project_slices_audio_destination
    ON project_slices (audio_destination)
    WHERE audio_destination IS NOT NULL;
