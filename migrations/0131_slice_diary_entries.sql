-- SKI-123 (P26 v2 K-03) — challenger diary on a slice.
--
-- Compagnonnage-style visibility: the process of doing a challenge is
-- itself a story worth showing. A challenger can post short markdown
-- entries during their work ("stuck on this test", "found a lead") that
-- other Skilluvers can read (with opt-out via is_public=false).
--
-- Kept intentionally simple:
--   - Only the current claimer can post (enforced at service layer).
--   - No edit history — small entries, high write cadence expected;
--     versioning would complicate more than it helps.
--   - Deleting the slice cascades entries (no orphan diaries).

CREATE TABLE IF NOT EXISTS slice_diary_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    author_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body_markdown TEXT NOT NULL CHECK (length(body_markdown) BETWEEN 1 AND 4000),
    is_public BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_slice_diary_by_slice
    ON slice_diary_entries (slice_id, created_at DESC);
