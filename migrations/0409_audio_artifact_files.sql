-- The files an audio artefact is made of, and what was measured in them.
--
-- ## Why a table and not columns on the slice
--
-- The backlog put format, sample rate and bit depth on `project_slices`. One
-- audio delivery is a master, eight stems and a compressed preview: three
-- different answers to each of those three questions. A single column would
-- have had to pick one and be wrong about the rest, and the first thing a
-- reviewer checks — "are the stems at the same rate as the master" — would
-- have been unaskable.
--
-- ## What is measured rather than declared
--
-- Duration, loudness, true peak, sample rate, bit depth and channel count all
-- come out of the file itself, by analysis, and are never taken from the
-- uploader's word. Two reasons, and the second is the important one:
--
--   * a declared loudness is the number somebody meant to hit, not the one
--     they hit, and the gap between those two is exactly what a reviewer is
--     looking for;
--   * the review grid of 0405 asks for loudness at the destination's norm. A
--     criterion that a reviewer has to measure by hand, on every file, is a
--     criterion that gets skipped.
--
-- `analysis_status` is what says whether the numbers are real yet. NULL
-- measurements with status `pending` means "not yet looked at", which is
-- honest; zero would mean silence.
--
-- ## Why the preview is a row and not a column
--
-- A thirty-second MP3 generated from a 200 MB master is a file: it has a
-- storage key, a size, a duration and a format like any other. Making it a
-- column would mean a second set of `preview_*` fields duplicating this
-- table, and `derived_from_id` says where it came from — which is also what
-- lets it be regenerated when the master is replaced.
--
-- ## Where the budgets live
--
-- In rows, per subtype. The backlog's figures are ops decisions that change
-- with what storage costs, and a hard-coded limit in Rust means a deployment
-- every time somebody needs to upload one FMOD project that is slightly too
-- big.

CREATE TABLE audio_artifact_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,

    -- What part this file plays in the delivery.
    role VARCHAR(20) NOT NULL CHECK (role IN (
        'master',           -- the finished thing, at full quality
        'stem',             -- one separated track of a master
        'preview',          -- a short, compressed excerpt, generated
        'project_archive',  -- an FMOD/Wwise/DAW project, or source code
        'documentation'     -- usage sheet, licence declaration, cue sheet
    )),
    -- The preview's origin. Set only on generated files, and what lets one be
    -- rebuilt when the master it came from is replaced.
    derived_from_id UUID REFERENCES audio_artifact_files(id) ON DELETE CASCADE,

    -- Where the bytes are, in the private bucket. Private and not public:
    -- unreleased work for a paying client is the normal case here, and a
    -- bucket that serves everything anonymously cannot hold it. Reading goes
    -- through a short-lived presigned URL.
    storage_key TEXT NOT NULL,
    original_filename VARCHAR(255) NOT NULL,
    byte_size BIGINT NOT NULL CHECK (byte_size > 0),
    container VARCHAR(10) NOT NULL CHECK (container IN (
        'wav', 'flac', 'aiff', 'mp3', 'ogg', 'opus', 'm4a', 'zip', 'pdf', 'md'
    )),

    -- Measured, never declared. NULL until the analysis has run, or when the
    -- file is not audio.
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms > 0),
    sample_rate_hz INTEGER CHECK (sample_rate_hz IS NULL OR sample_rate_hz > 0),
    bit_depth SMALLINT CHECK (bit_depth IS NULL OR bit_depth > 0),
    channels SMALLINT CHECK (channels IS NULL OR channels > 0),
    -- Integrated loudness, in LUFS. Negative in every real case.
    loudness_lufs NUMERIC(5,2),
    -- True peak in dBFS. Above 0 means it will clip on some decoders, which is
    -- a finding rather than an impossibility, so it is not constrained.
    true_peak_dbfs NUMERIC(5,2),
    -- Loudness range, in LU. The dynamic room the piece actually uses.
    loudness_range_lu NUMERIC(5,2),
    -- Peaks for drawing, as a compact array. Held rather than recomputed: a
    -- profile page draws dozens of these and re-reading the audio for each
    -- would be the slowest thing on the page.
    waveform_peaks JSONB,

    analysis_status VARCHAR(12) NOT NULL DEFAULT 'pending'
        CHECK (analysis_status IN ('pending', 'done', 'failed', 'skipped')),
    analysis_error TEXT,
    analysed_at TIMESTAMPTZ,

    sort_order SMALLINT NOT NULL DEFAULT 100,
    uploaded_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A generated file has an origin; an uploaded one does not.
    CONSTRAINT audio_files_only_a_preview_is_derived CHECK (
        derived_from_id IS NULL OR role = 'preview'
    ),
    CONSTRAINT audio_files_a_preview_is_not_its_own_origin CHECK (
        derived_from_id IS NULL OR derived_from_id <> id
    ),
    -- A failed analysis has to say why. Otherwise the only way to know what
    -- went wrong is to try again and watch.
    CONSTRAINT audio_files_failure_states_a_reason CHECK (
        analysis_status <> 'failed' OR btrim(COALESCE(analysis_error, '')) <> ''
    ),
    CONSTRAINT audio_files_storage_key_is_unique UNIQUE (storage_key)
);

COMMENT ON TABLE audio_artifact_files IS
    'Every file of an audio delivery, with what the analysis measured in it. '
    'A table rather than columns on project_slices: a master, its stems and a '
    'preview have three different sample rates, and the first question a '
    'reviewer asks is whether they match.';

COMMENT ON COLUMN audio_artifact_files.loudness_lufs IS
    'Integrated loudness, measured from the file. Never taken from the '
    'uploader: the declared figure is the one they aimed at, and the gap '
    'between that and the one they hit is what the review grid asks about.';

CREATE INDEX idx_audio_files_by_slice
    ON audio_artifact_files (slice_id, role, sort_order);

CREATE INDEX idx_audio_files_pending_analysis
    ON audio_artifact_files (created_at)
    WHERE analysis_status = 'pending';

CREATE TRIGGER trg_audio_artifact_files_updated_at
    BEFORE UPDATE ON audio_artifact_files
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- A file belongs to an audio slice. Nothing else has files of this shape, and
-- an effects pack hanging off a Figma frame is a bug that would only surface
-- when somebody tried to play it.
CREATE FUNCTION trg_audio_files_belong_to_an_audio_slice() RETURNS TRIGGER AS $$
DECLARE
    kind VARCHAR;
BEGIN
    SELECT slice_type INTO kind FROM project_slices WHERE id = NEW.slice_id;
    IF kind <> 'audio_artifact' THEN
        RAISE EXCEPTION 'slice % is a %, not an audio_artifact', NEW.slice_id, kind
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_audio_files_belong_to_an_audio_slice
    BEFORE INSERT OR UPDATE OF slice_id ON audio_artifact_files
    FOR EACH ROW EXECUTE FUNCTION trg_audio_files_belong_to_an_audio_slice();

-- ═══════════════════════════════════════════════════════════════════
-- What each kind of delivery is allowed to weigh
-- ═══════════════════════════════════════════════════════════════════
--
-- The figures come from the backlog, and the reason they are rows is that
-- they are the kind of number somebody needs to change on a Tuesday: an FMOD
-- project that is six percent over a limit should cost a support message, not
-- a release.
--
-- `max_total_bytes` is the sum over one slice, not per file. A pack is fifty
-- small files and a system is one large one, and a per-file cap would have to
-- be generous enough for the second — which makes it meaningless for the
-- first.

CREATE TABLE audio_upload_budgets (
    audio_subtype VARCHAR(30) PRIMARY KEY,
    max_total_bytes BIGINT NOT NULL CHECK (max_total_bytes > 0),
    max_files SMALLINT NOT NULL CHECK (max_files > 0),
    rationale TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO audio_upload_budgets (audio_subtype, max_total_bytes, max_files, rationale) VALUES
    ('composition',           500 * 1024 * 1024, 40,
     'Un master et ses stems. Quarante fichiers couvrent une pièce orchestrale découpée par pupitre.'),
    ('sound_pack',            500 * 1024 * 1024, 200,
     'Un pack est nombreux et léger : deux cents fichiers courts tiennent largement dans la limite.'),
    ('voice_reel',            100 * 1024 * 1024, 20,
     'Une bande démo et ses extraits. Rien ici ne dure longtemps.'),
    ('adaptive_music_system', 2048::BIGINT * 1024 * 1024, 60,
     'Un projet FMOD ou Wwise embarque ses sources. C''est la livraison la plus lourde du domaine.'),
    ('audio_programming',     100 * 1024 * 1024, 40,
     'Du code et quelques échantillons de démonstration.'),
    ('ambient_soundscape',    1024::BIGINT * 1024 * 1024, 30,
     'Des boucles longues, souvent non compressées.');

CREATE TRIGGER trg_audio_upload_budgets_updated_at
    BEFORE UPDATE ON audio_upload_budgets
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();
