-- Playtests: the game domain's first-class evidence.
--
-- This is the one genuinely new object the domain needs, the way
-- security_findings was for security. Every other domain validates on a
-- reviewer's judgement and a passing build; game does not accept "it runs and I
-- like it" as enough. A game artefact reaches `validated` only after real
-- players have touched it — the rule the review grids state and this table
-- records.
--
-- ## Two tables
--
-- `game_playtests` is one player's session: how long, how it felt, what broke,
-- whether they would return. One row per (slice, playtester) — a person's
-- verdict is one verdict, editable, not a ballot box.
--
-- `game_playtest_recruitments` is a creator asking for testers on a slice: an
-- open call the community matching the orientation can answer. Kept separate so
-- a recruitment can exist before any playtest does, and close when enough have
-- come in.

CREATE TABLE game_playtest_recruitments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    -- The creator who opened it, so only they can close it.
    opened_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- What a tester is told and where the build is. The build URL is the slice's
    -- own playable URL by default, but a recruitment can point somewhere else
    -- (a private itch page, a time-limited link).
    build_url VARCHAR(500) NOT NULL,
    brief_md TEXT NOT NULL,
    -- How many are wanted. The domain floor is three; a creator can ask for more.
    testers_wanted SMALLINT NOT NULL DEFAULT 3 CHECK (testers_wanted >= 3),
    -- Anonymous testing is a choice the creator offers, not a default.
    allows_anonymous BOOLEAN NOT NULL DEFAULT FALSE,
    opened_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    closed_at TIMESTAMPTZ,
    -- One open recruitment per slice at a time.
    CONSTRAINT one_open_recruitment_per_slice
        EXCLUDE (slice_id WITH =) WHERE (closed_at IS NULL)
);

CREATE INDEX idx_game_playtest_recruitments_open
    ON game_playtest_recruitments (slice_id) WHERE closed_at IS NULL;

CREATE TABLE game_playtests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    playtester_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Nullable: a session that was cut short still says something, and a
    -- creator would rather know it ran two minutes than not know it ran.
    session_duration_min SMALLINT CHECK (session_duration_min IS NULL OR session_duration_min >= 0),
    -- The two scores the validation rule reads: fun is the bar (average >= 3
    -- across at least three testers), clarity is the diagnostic.
    fun_score SMALLINT NOT NULL CHECK (fun_score BETWEEN 1 AND 5),
    clarity_score SMALLINT NOT NULL CHECK (clarity_score BETWEEN 1 AND 5),
    difficulty_perception VARCHAR(12) NOT NULL
        CHECK (difficulty_perception IN ('too_easy', 'balanced', 'too_hard', 'unclear')),
    bugs_encountered_md TEXT,
    suggestions_md TEXT,
    would_play_again BOOLEAN NOT NULL,
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- One verdict per person per slice. Editable in place, not stackable.
    UNIQUE (slice_id, playtester_user_id)
    -- A creator playtesting their own slice is refused in the service, which
    -- already loads the slice to find its author; a CHECK here would need the
    -- author denormalised onto every playtest for no gain.
);

CREATE INDEX idx_game_playtests_slice ON game_playtests (slice_id, submitted_at DESC);
CREATE INDEX idx_game_playtests_tester ON game_playtests (playtester_user_id);

COMMENT ON TABLE game_playtests IS
    'One playtester''s verdict on a game slice. A slice reaches validated only '
    'with at least three of these and a fun_score average of 3 or more — the '
    'rule lives in the game service, not in a trigger, because it also has to '
    'issue the attestation and the fragments at the same moment.';
