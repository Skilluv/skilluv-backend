-- Where design work comes from.
--
-- ## Why design needs this and code does not
--
-- A code challenge arrives on its own: somebody opens an issue on a
-- repository, the ingestor reads the label, a slice appears. There is no
-- equivalent for design. Nobody files "the contrast on this settings page is
-- unreadable" as a ticket with a `design` label, and the projects that would
-- benefit most are the ones with no designer to notice.
--
-- So the source is editorial. Somebody writes a brief, somebody else reads it,
-- and it becomes work. This table is that queue.
--
-- ## Why a proposal is not a challenge template
--
-- `challenge_templates` already has a community-proposal flow, and the design
-- catalogue seeds a hundred and thirty of them. Those are *exercises*: a
-- prompt, an evaluation rubric, no claimant and no critique loop.
--
-- A design brief is the other thing. It becomes a `project_slices` row of type
-- `design_artifact`, because that is what the review loop runs on — versions,
-- rounds, blocking reasons, a reviewer resolved from the trade. A template
-- carries none of that: no orientation, no subtype, no expected rounds, no
-- reviewer family. Adding those columns to `challenge_templates` would give it
-- a second identity and leave every existing reader guessing which one it has.
--
-- ## Why the proposal survives publication
--
-- `published_slice_id` points at what it became, and the row stays. Who wrote
-- the brief is not recorded anywhere on the slice — the slice records who
-- *did* the work — and the author of a brief that produced good work deserves
-- to be findable. It is also what the `design-brief-author` badge counts.

CREATE TABLE design_brief_proposals (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    proposed_by   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    title         VARCHAR(160) NOT NULL CHECK (length(title) BETWEEN 8 AND 160),
    -- The brief itself, following one of the thirteen family templates in
    -- `docs/design/BRIEF-TEMPLATES.md`. The floor is deliberate: a brief of
    -- two lines produces answers to different questions, and the reviewer
    -- arbitrates on taste.
    brief_md      TEXT NOT NULL CHECK (length(brief_md) BETWEEN 200 AND 20000),

    -- Which trade, which kind of artefact. Both required: a brief that names
    -- neither cannot be routed to anybody competent, which is the one thing a
    -- brief has to do.
    orientation_id UUID NOT NULL REFERENCES orientations(id) ON DELETE RESTRICT,
    design_subtype VARCHAR(30) NOT NULL,

    difficulty     SMALLINT NOT NULL CHECK (difficulty BETWEEN 1 AND 5),
    estimated_hours INTEGER CHECK (estimated_hours IS NULL OR estimated_hours > 0),
    -- How many critique rounds the brief announces. The hard ceiling is five
    -- and lives on the decision journal; this is the promise.
    expected_rounds SMALLINT CHECK (expected_rounds IS NULL OR expected_rounds BETWEEN 1 AND 5),

    -- Claimed by one person, or answered by many. Two different weeks, and
    -- the brief has to say which before anybody starts.
    format         VARCHAR(20) NOT NULL DEFAULT 'individual'
        CHECK (format IN ('individual', 'contest')),

    status         VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'published', 'rejected', 'withdrawn')),
    -- Why it was refused, shown to the author. A refusal with no reason is a
    -- refusal that comes back.
    review_feedback TEXT,
    reviewed_by     UUID REFERENCES users(id) ON DELETE SET NULL,
    reviewed_at     TIMESTAMPTZ,
    -- What it became.
    published_slice_id UUID REFERENCES project_slices(id) ON DELETE SET NULL,

    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT design_brief_rejection_says_why
        CHECK (status <> 'rejected' OR (review_feedback IS NOT NULL AND length(review_feedback) >= 20)),
    CONSTRAINT design_brief_published_names_its_slice
        CHECK (status <> 'published' OR published_slice_id IS NOT NULL)
);

COMMENT ON TABLE design_brief_proposals IS
    'Briefs waiting to become design work. Design has no ingestion source the '
    'way code has GitHub issues, so the source is editorial: somebody writes a '
    'brief, somebody reads it, it becomes a slice.';

COMMENT ON COLUMN design_brief_proposals.published_slice_id IS
    'What the brief became. The row stays after publication: the slice records '
    'who did the work, not who set it, and the author of a brief that produced '
    'good work deserves to be findable.';

-- The review queue: oldest first, so nobody waits twice.
CREATE INDEX idx_design_briefs_pending
    ON design_brief_proposals (created_at ASC)
    WHERE status = 'pending';

-- Somebody's own briefs, and the badge's count.
CREATE INDEX idx_design_briefs_by_author
    ON design_brief_proposals (proposed_by, status);

-- ═══════════════════════════════════════════════════════════════════
-- The badge
-- ═══════════════════════════════════════════════════════════════════
--
-- Five published briefs. Setting work for other people is a contribution the
-- platform has no other way of recognising: it leaves no deliverable, earns no
-- craft score, and is invisible on a profile — which is exactly how a
-- community runs out of briefs.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity)
VALUES (
    'design-brief-author',
    'medal',
    'Auteur de briefs',
    'Cinq briefs design publiés — du travail posé pour d''autres.',
    '{"proof_types": ["design_briefs_published"], "min_count": 5}',
    'rare'
)
ON CONFLICT (slug) DO NOTHING;
