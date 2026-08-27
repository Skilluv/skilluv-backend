-- The game slice type, and the columns a game artefact needs.
--
-- `slice_type` has been a row in `slice_types` since 0413, not a value in a
-- CHECK — the W-01 ticket predates that and its SQL is stale. So the type is
-- inserted, and the game-specific fields are added to `project_slices` behind a
-- coherence constraint, the same way security did in 0550: a game field on a
-- non-game slice, or a game slice without a subtype, is refused rather than
-- left to mean nothing.

INSERT INTO slice_types (slug, skill_domain, name, description, sort_order) VALUES
('game_artifact', 'game', 'Game artefact',
 'A piece of a game delivered on its own: a code module or a playable build, a '
 'design document, a 3D or 2D asset, an animation pack, a level pack, or a mod. '
 'Its `game_artifact_subtype` says which, and that decides how it is stored, '
 'previewed and reviewed.', 700)
ON CONFLICT (slug) DO NOTHING;

ALTER TABLE project_slices
    -- Which of the eight kinds of game deliverable this is. The storage limit,
    -- the preview generator and the review grid all branch on it.
    ADD COLUMN game_artifact_subtype VARCHAR(20)
        CHECK (game_artifact_subtype IS NULL OR game_artifact_subtype IN (
            'code_module',      -- gameplay or engine code
            'build_playable',   -- an executable: Win/Mac/Linux/WebGL/APK
            'gdd_document',     -- a game design document, RFC or spec
            'asset_3d',         -- .fbx/.blend/.gltf plus textures
            'asset_2d_sprite',  -- sprite sheets and atlases
            'animation_pack',   -- rigs and animations (VFX included)
            'level_pack',       -- levels in an engine's format
            'mod_package'       -- a mod, hosted on a third-party platform
        )),
    -- The engine, so a reviewer knows what conventions to read against and a
    -- build check knows what to run. 'n/a' for a document or an external mod.
    ADD COLUMN game_engine VARCHAR(12)
        CHECK (game_engine IS NULL OR game_engine IN (
            'godot', 'unity', 'unreal', 'bevy', 'love2d',
            'phaser', 'gamemaker', 'custom', 'n/a'
        )),
    ADD COLUMN game_target_platforms TEXT[] NOT NULL DEFAULT '{}',
    -- Where the thing is playable: an itch embed, a WebGL build URL, an
    -- external mod page. NULL for a source-only or document deliverable.
    ADD COLUMN game_playable_url VARCHAR(500),
    -- How it was made: alone, in a jam under a clock, or as one role of a
    -- multi-artefact team project. Only game artefacts carry it.
    ADD COLUMN game_challenge_format VARCHAR(12)
        CHECK (game_challenge_format IS NULL OR game_challenge_format IN (
            'individual', 'jam', 'team_project'
        ));

ALTER TABLE project_slices
    -- A game slice has a subtype; nothing else does. One column decides whether
    -- the row is a game artefact at all.
    ADD CONSTRAINT project_slices_game_subtype_belongs CHECK (
        (slice_type = 'game_artifact') = (game_artifact_subtype IS NOT NULL)
    ),
    -- And a game artefact always says how it was made — the jam and team
    -- formats change what a reviewer expects, so the format is not optional
    -- once the slice is a game one.
    ADD CONSTRAINT a_game_artefact_names_its_format CHECK (
        slice_type <> 'game_artifact' OR game_challenge_format IS NOT NULL
    ),
    -- Engine and platforms are about a game artefact; they must be empty on
    -- anything else, so a stray value cannot pretend the row is game work.
    ADD CONSTRAINT game_engine_belongs CHECK (
        slice_type = 'game_artifact' OR game_engine IS NULL
    ),
    ADD CONSTRAINT game_platforms_belong CHECK (
        slice_type = 'game_artifact' OR game_target_platforms = '{}'
    );

CREATE INDEX idx_project_slices_game_subtype
    ON project_slices (game_artifact_subtype)
    WHERE game_artifact_subtype IS NOT NULL;
