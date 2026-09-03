-- Bonjour Skilluv stops being a GitHub fork.
--
-- ## What it was
--
-- `POST /api/onboarding/bonjour-skilluv/start` loaded the caller's GitHub
-- token and answered 400 "GitHub account not connected" when there was none.
-- Then it forked `skilluv-community/starter-*` and waited for a webhook.
--
-- That is the right first gesture for `code`, and it is a wall for everybody
-- else. The very first thing the platform asked of a designer, a sound
-- engineer, a teacher or somebody arriving at leadership was: open a GitHub
-- account and file a pull request. They leave before proving anything, having
-- learnt that this place is not for them (SKI-362).
--
-- ## What it is
--
-- One rite, twelve gestures. The name, the badge and the moment stay single;
-- the shape is the trade's. Migration 0607 wrote the twelve briefs and marks
-- each with `is_domain_rite`; this is the tracking table catching up with
-- them.
--
-- Two forms, because there are two mechanically distinct things to track and
-- not twelve:
--
--   `fork`       — the code gesture. A repository on GitHub, a pull request,
--                  a webhook. Every column this table already had.
--   `submission` — the other eleven. An artifact handed in against the
--                  domain's rite template, landing in the human review queue
--                  through the path SKI-361 opened. What identifies it is a
--                  `challenge_submissions` row, not a fork id.
--
-- The eleven differ in what they ask for — a screen, a playtest verdict, a
-- finding, twenty seconds of sound — and that difference lives in the brief a
-- person reads and in `services::onboarding_rite`, which is what the front
-- renders. It is not a column here: this table records that the rite was
-- started, handed in, and passed, and those three facts have the same shape
-- for a translation and for a defect report.
--
-- ## Why the GitHub columns become nullable rather than moving to a sister table
--
-- A sister table would have to be joined by every reader for a row that has
-- one of the two shapes, and `github_fork_id UNIQUE` would still have to live
-- somewhere. Nullable columns plus a form discriminator and a coherence check
-- per form say the same thing in one place: a `submission` row carries no fork
-- columns at all, and the check refuses one that does.
--
-- ## Existing rows
--
-- Every row that exists is a fork — nothing else could be created. The
-- defaults (`rite_form = 'fork'`, `skill_domain = 'code'`) describe them
-- exactly, so the backfill is the default and there is no data migration.

-- ═══════════════════════════════════════════════════════════════════
-- 1. The form, the domain, and what a non-fork rite points at
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE onboarding_bonjour_skilluv
    ADD COLUMN rite_form VARCHAR(20) NOT NULL DEFAULT 'fork'
        CHECK (rite_form IN ('fork', 'submission')),
    ADD COLUMN skill_domain VARCHAR(20) NOT NULL DEFAULT 'code'
        REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    -- The rite's brief, so the row knows which template it is against even if
    -- a curator later publishes a different one for the domain.
    ADD COLUMN challenge_id UUID REFERENCES challenge_templates(id) ON DELETE SET NULL,
    -- What was handed in, for a `submission` rite. NULL until it is.
    ADD COLUMN submission_id UUID REFERENCES challenge_submissions(id) ON DELETE SET NULL;

ALTER TABLE onboarding_bonjour_skilluv
    ALTER COLUMN starter_slug DROP NOT NULL,
    ALTER COLUMN fork_full_name DROP NOT NULL,
    ALTER COLUMN fork_html_url DROP NOT NULL,
    ALTER COLUMN github_fork_id DROP NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- 2. Statuses: a submission rite has its own three
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE onboarding_bonjour_skilluv
    DROP CONSTRAINT onboarding_bonjour_skilluv_status_check;

ALTER TABLE onboarding_bonjour_skilluv
    ADD CONSTRAINT onboarding_bonjour_skilluv_status_check
    CHECK (status IN (
        -- fork form
        'forked', 'hello_committed', 'pr_opened',
        -- submission form
        'started', 'submitted',
        -- both
        'completed', 'abandoned'
    ));

-- ═══════════════════════════════════════════════════════════════════
-- 3. Coherence, per form
-- ═══════════════════════════════════════════════════════════════════

-- The two table-level checks written in 0111 are anonymous, so Postgres named
-- them `onboarding_bonjour_skilluv_check` and `..._check1` — an order this
-- migration would rather not depend on. Dropped by what they say instead of by
-- what they happen to be called.
DO $$
DECLARE
    c RECORD;
BEGIN
    FOR c IN
        SELECT conname
        FROM pg_constraint
        WHERE conrelid = 'onboarding_bonjour_skilluv'::regclass
          AND contype = 'c'
          AND (pg_get_constraintdef(oid) LIKE '%pr_number%'
               OR pg_get_constraintdef(oid) LIKE '%completed_at%')
    LOOP
        EXECUTE format('ALTER TABLE onboarding_bonjour_skilluv DROP CONSTRAINT %I', c.conname);
    END LOOP;
END $$;

ALTER TABLE onboarding_bonjour_skilluv
    -- A fork rite has a fork, and reaches `pr_opened` only with a pull request.
    ADD CONSTRAINT onboarding_bonjour_fork_shape CHECK (
        rite_form <> 'fork'
        OR (
            starter_slug IS NOT NULL
            AND fork_full_name IS NOT NULL
            AND fork_html_url IS NOT NULL
            AND github_fork_id IS NOT NULL
            AND (
                (status IN ('forked', 'hello_committed')
                    AND pr_number IS NULL AND pr_url IS NULL)
                OR (status IN ('pr_opened', 'completed')
                    AND pr_number IS NOT NULL AND pr_url IS NOT NULL)
                OR status = 'abandoned'
            )
        )
    ),
    -- A submission rite has no GitHub anything. This is the check that makes
    -- the endpoint's promise structural rather than a matter of the handler
    -- remembering.
    ADD CONSTRAINT onboarding_bonjour_submission_shape CHECK (
        rite_form <> 'submission'
        OR (
            starter_slug IS NULL
            AND fork_full_name IS NULL
            AND fork_html_url IS NULL
            AND github_fork_id IS NULL
            AND pr_number IS NULL
            AND pr_url IS NULL
            AND status IN ('started', 'submitted', 'completed', 'abandoned')
        )
    ),
    ADD CONSTRAINT onboarding_bonjour_completed_at CHECK (
        completed_at IS NULL OR status = 'completed'
    );

CREATE INDEX idx_onboarding_bonjour_submission
    ON onboarding_bonjour_skilluv (submission_id)
    WHERE submission_id IS NOT NULL;

COMMENT ON COLUMN onboarding_bonjour_skilluv.rite_form IS
    'fork = the code gesture (a starter forked on GitHub, a pull request, a '
    'webhook). submission = the other eleven domains — an artifact handed in '
    'against the domain''s is_domain_rite template, read by a person in the '
    'review queue. Which form a domain takes is declared in '
    'services::onboarding_rite, not here.';

COMMENT ON COLUMN onboarding_bonjour_skilluv.status IS
    'fork form: forked -> (hello_committed) -> pr_opened -> completed. '
    'submission form: started -> submitted -> completed, where submitted means '
    'the artifact is in the review queue and completed means a reviewer '
    'approved it. abandoned in both, inferred by a cleanup job after 90 idle '
    'days.';
