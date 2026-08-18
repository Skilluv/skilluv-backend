-- Handing in a file that is too big to send through an API.
--
-- ## Why the bytes never touch the backend
--
-- The design backlog described `POST /api/design/artifacts/upload` taking
-- multipart chunks. Five gigabytes through an axum handler on a small VPS is
-- how the API falls over, and it falls over for everybody at once — the
-- upload holds a connection and a buffer for as long as somebody's rural
-- connection takes to push a Blender scene.
--
-- So the object store does the receiving. The backend hands out presigned PUT
-- URLs for each part, the client uploads straight to MinIO, and the backend
-- sees only the part list at the end. That is also what makes the upload
-- resumable for free: a part that failed is one presigned URL away from being
-- retried, and nothing on our side has to remember a byte offset.
--
-- ## Why a session table rather than a bare S3 upload
--
-- S3 already tracks an unfinished multipart upload. What it does not know is
-- *whose* it is, which slice it belongs to, or what the person said they were
-- sending — and those are exactly what a ceiling is checked against, and what
-- a sweep needs in order to abandon the ones nobody finished.
--
-- ## Why the declared size is kept beside the real one
--
-- The ceiling is checked twice: at `init` against what the client says, and
-- at `complete` against what the object store actually holds. The first is
-- what stops a five-gigabyte upload from starting; the second is what stops a
-- client that lied. Neither alone is enough, and keeping both is what lets
-- somebody see that a client is lying.
--
-- ## Why no preview generator
--
-- The ticket asked for server-side previews: ffmpeg for video, Blender
-- headless for 3D, a thumbnailer for images, all spawned through a Docker
-- socket. That is three heavy binaries and a privileged socket on a machine
-- this project cannot afford, to produce a still frame that the person who
-- made the file could pick better than any heuristic.
--
-- The ticket already concedes the principle for After Effects projects — "a
-- separately uploaded MP4 preview is required" — because nothing can parse an
-- `.aep`. This applies that rule to every subtype whose source file a browser
-- cannot open: the preview is supplied, not rendered, and `preview_key` here
-- is where it lands.

CREATE TABLE design_upload_sessions (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Which challenge this is for. Nullable: somebody can upload before
    -- deciding, and a contest entry belongs to no slice at all.
    slice_id      UUID REFERENCES project_slices(id) ON DELETE SET NULL,

    -- What is being sent. The subtype decides the ceiling and whether a
    -- preview is required, so it is not optional.
    design_subtype VARCHAR(30) NOT NULL,
    filename       VARCHAR(255) NOT NULL CHECK (length(filename) BETWEEN 1 AND 255),
    content_type   VARCHAR(120) NOT NULL,

    -- What the client says it is sending, and what the store actually held
    -- once it was done. Both, for the reason above.
    declared_bytes BIGINT NOT NULL CHECK (declared_bytes > 0),
    stored_bytes   BIGINT CHECK (stored_bytes IS NULL OR stored_bytes > 0),

    part_size      INTEGER NOT NULL CHECK (part_size >= 5 * 1024 * 1024),
    part_count     INTEGER NOT NULL CHECK (part_count BETWEEN 1 AND 10000),

    -- Where it lands in the private bucket, and the object store's own handle
    -- on the unfinished upload.
    storage_key    TEXT NOT NULL UNIQUE,
    s3_upload_id   TEXT NOT NULL,

    -- Where the supplied preview lands, when the subtype requires one.
    preview_key    TEXT,

    status         VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'completed', 'aborted', 'expired')),

    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at   TIMESTAMPTZ,
    -- After this, the sweep abandons it at the object store and stops paying
    -- for the parts already there.
    expires_at     TIMESTAMPTZ NOT NULL,

    CONSTRAINT design_upload_completed_has_a_size
        CHECK (status <> 'completed' OR (stored_bytes IS NOT NULL AND completed_at IS NOT NULL))
);

COMMENT ON TABLE design_upload_sessions IS
    'An in-progress hand-in of a file too large to send through the API. The '
    'bytes go straight to the object store through presigned part URLs; this '
    'row is what says whose they are and what was promised.';

COMMENT ON COLUMN design_upload_sessions.declared_bytes IS
    'What the client said it was sending. Checked at init to refuse an upload '
    'that would exceed the subtype ceiling before a byte moves; compared with '
    '`stored_bytes` at completion to catch a client that lied.';

COMMENT ON COLUMN design_upload_sessions.preview_key IS
    'A supplied preview, not a rendered one. Nothing here parses an .aep or a '
    '.blend; the person who made the file picks the frame that represents it.';

-- The sweep: unfinished sessions past their expiry, oldest first.
CREATE INDEX idx_design_uploads_expiring
    ON design_upload_sessions (expires_at)
    WHERE status = 'pending';

-- Somebody's own uploads, newest first.
CREATE INDEX idx_design_uploads_by_user
    ON design_upload_sessions (user_id, created_at DESC);
