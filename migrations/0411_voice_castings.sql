-- Casting a voice: a call, takes, and one choice.
--
-- ## Why this is not a contest
--
-- `tournaments` already runs "several people submit, somebody ranks them",
-- and a casting looks like that from a distance. It is not, on three counts
-- that all matter to the people involved:
--
--   * **there is no ranking.** A casting produces one selected voice, not a
--     podium. Second place in a casting is not a result, it is a no.
--   * **the brief is a character, not a problem.** What is judged is fit —
--     this voice for this part — and the same take can be right for one part
--     and wrong for the next. A contest score would claim something general
--     about the person that nobody meant.
--   * **the person who chooses is the one who has to live with it.** A jury
--     can advise; the creator decides. `tournaments` has no notion of an
--     entrant whose opinion outranks the jury's.
--
-- ## Blind by default
--
-- The listener sees the take and not the name until a choice is made. Casting
-- is the single place on this platform where an established reputation most
-- directly competes with the thing being judged, and where the judgement is
-- most nearly instant — thirty seconds of audio, decided in ten. The default
-- is the one that gives an unknown voice a hearing.
--
-- It is a default, not a rule: a creator recasting a returning character
-- already knows whose voice they need, and forcing blindness there would be
-- theatre. `is_blind` is on the casting, and turning it off is a choice
-- somebody makes visibly.

CREATE TABLE voice_castings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The work the voice is for.
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    opened_by UUID REFERENCES users(id) ON DELETE SET NULL,

    -- Who this character is: age, register, situation, what they want. The
    -- part an actor actually needs in order to arrive with an idea.
    character_brief_md TEXT NOT NULL CHECK (btrim(character_brief_md) <> ''),
    -- The lines to read. Everybody reads the same ones, or the takes are not
    -- comparable.
    sample_line_text TEXT NOT NULL CHECK (btrim(sample_line_text) <> ''),
    -- BCP-47, so 'fr', 'fr-BE', 'en-GB' are distinguishable — an accent is
    -- part of the brief here, not a detail.
    target_language VARCHAR(20) NOT NULL,
    -- What a usable audition weighs, in seconds. Stated so a rejection for
    -- length is a rule rather than a taste.
    max_audition_seconds SMALLINT NOT NULL DEFAULT 90
        CHECK (max_audition_seconds BETWEEN 10 AND 600),

    is_blind BOOLEAN NOT NULL DEFAULT TRUE,
    audition_deadline TIMESTAMPTZ NOT NULL,

    status VARCHAR(20) NOT NULL DEFAULT 'open' CHECK (status IN (
        'open',       -- taking auditions
        'reviewing',  -- deadline passed, listening
        'selected',   -- a voice was chosen
        'cancelled'   -- the part went away
    )),
    selected_submission_id UUID,
    selected_at TIMESTAMPTZ,
    cancellation_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A selection is a submission and a moment, or neither.
    CONSTRAINT voice_casting_selection_is_complete CHECK (
        (selected_submission_id IS NULL) = (selected_at IS NULL)
    ),
    CONSTRAINT voice_casting_selected_means_selected CHECK (
        status <> 'selected' OR selected_submission_id IS NOT NULL
    ),
    CONSTRAINT voice_casting_cancellation_says_why CHECK (
        status <> 'cancelled' OR btrim(COALESCE(cancellation_reason, '')) <> ''
    )
);

COMMENT ON TABLE voice_castings IS
    'A call for a voice, its takes, and the one choice at the end. Not a '
    'tournament: there is no ranking, the brief is a character rather than a '
    'problem, and the creator outranks the jury.';

COMMENT ON COLUMN voice_castings.is_blind IS
    'Whether names are hidden until a choice is made. TRUE by default: thirty '
    'seconds of audio judged in ten is where reputation most easily replaces '
    'listening. Turning it off is visible.';

CREATE INDEX idx_voice_castings_open
    ON voice_castings (audition_deadline)
    WHERE status = 'open';

CREATE INDEX idx_voice_castings_by_slice ON voice_castings (slice_id);

CREATE TRIGGER trg_voice_castings_updated_at
    BEFORE UPDATE ON voice_castings
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The takes
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE voice_audition_submissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    casting_id UUID NOT NULL REFERENCES voice_castings(id) ON DELETE CASCADE,
    voice_actor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- Where the take is. Uploaded through the platform, or hosted by the
    -- actor — both happen, and refusing the second would exclude everybody
    -- whose demo already lives on a service they pay for.
    audition_storage_key TEXT,
    audition_url TEXT,

    -- Measured on upload, when we hold the file. NULL for a hosted take: we
    -- are not going to claim numbers we did not compute.
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms > 0),
    loudness_lufs NUMERIC(5,2),
    true_peak_dbfs NUMERIC(5,2),

    -- What the actor wants heard: the interpretation chosen, the alternative
    -- offered, the line they would say differently.
    notes_md TEXT,

    withdrawn_at TIMESTAMPTZ,
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT voice_audition_says_where_it_is CHECK (
        btrim(COALESCE(audition_storage_key, '')) <> ''
        OR btrim(COALESCE(audition_url, '')) <> ''
    )
);

COMMENT ON TABLE voice_audition_submissions IS
    'One take per actor per casting. A second take replaces the first rather '
    'than joining it: a listener comparing two versions of the same voice is '
    'doing the actor no favour, and the actor chose which one to send.';

-- One live take per actor per casting.
CREATE UNIQUE INDEX uniq_voice_audition_per_actor
    ON voice_audition_submissions (casting_id, voice_actor_user_id)
    WHERE withdrawn_at IS NULL;

CREATE INDEX idx_voice_auditions_by_casting
    ON voice_audition_submissions (casting_id, submitted_at);

ALTER TABLE voice_castings
    ADD CONSTRAINT voice_castings_selected_submission_fkey
    FOREIGN KEY (selected_submission_id)
    REFERENCES voice_audition_submissions(id) ON DELETE SET NULL;
